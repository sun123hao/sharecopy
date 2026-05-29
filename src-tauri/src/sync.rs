use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::clipboard::{ClipboardContent, ClipboardWatcher};
use crate::discovery::{DiscoveryEvent, DiscoveryService};
use crate::network::{NetworkEvent, NetworkManager};
use crate::protocol::{ClipboardImageChunkPayload, ClipboardImagePayload, ClipboardTextPayload, ImageFormat, Message};
use crate::transfer::FileTransferManager;

pub const LARGE_IMAGE_THRESHOLD: usize = 10 * 1024 * 1024; // 10MB
pub const IMAGE_CHUNK_SIZE: usize = 512 * 1024; // 512KB

// ── 同步统计 ──────────────────────────────
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncStats {
    pub texts_synced: u64,
    pub images_synced: u64,
    pub files_transferred: u64,
}

// ── 剪贴板历史条目 ──────────────────────────
const MAX_HISTORY_ENTRIES: usize = 50;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipboardHistoryEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub entry_type: String, // "text" 或 "image"
    pub content: String,    // 文本内容或图片 base64
    pub from_device: String,
    pub timestamp: u64,
}

// ── 图片块组装器 ──────────────────────────────
#[derive(Debug)]
struct ImageChunkAssembler {
    transfer_id: String,
    width: u32,
    height: u32,
    total_chunks: u16,
    chunks: Vec<Option<Vec<u8>>>,
    received: u16,
}

impl ImageChunkAssembler {
    fn new(transfer_id: String, width: u32, height: u32, total_chunks: u16) -> Self {
        Self {
            transfer_id,
            width,
            height,
            total_chunks,
            chunks: (0..total_chunks).map(|_| None).collect(),
            received: 0,
        }
    }

    fn add_chunk(&mut self, index: u16, data: Vec<u8>) {
        if index < self.total_chunks && self.chunks[index as usize].is_none() {
            self.chunks[index as usize] = Some(data);
            self.received += 1;
        }
    }

    fn is_complete(&self) -> bool {
        self.received == self.total_chunks
    }

    fn assemble(self) -> ClipboardContent {
        let data: Vec<u8> = self.chunks.into_iter().flatten().flatten().collect();
        ClipboardContent::Image {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

// ── 同步引擎 ──────────────────────────────
pub struct SyncEngine {
    device_id: String,
    watcher: Arc<ClipboardWatcher>,
    #[allow(dead_code)]
    discovery: Arc<std::sync::Mutex<DiscoveryService>>,
    network: Arc<NetworkManager>,
    transfer: Arc<FileTransferManager>,
    sync_enabled: Arc<AtomicBool>,
    stats: Arc<parking_lot::Mutex<SyncStats>>,
    history: Arc<parking_lot::Mutex<Vec<ClipboardHistoryEntry>>>,
    image_assemblers: Arc<parking_lot::Mutex<Vec<ImageChunkAssembler>>>,
    /// 无连接时缓存的最新剪贴板内容，设备连接后立即广播
    pending_clipboard: Arc<parking_lot::Mutex<Option<(ClipboardContent, String, std::time::Instant)>>>,
    app_handle: AppHandle,
}

impl SyncEngine {
    pub fn new(
        device_id: String,
        watcher: Arc<ClipboardWatcher>,
        discovery: DiscoveryService,
        network: Arc<NetworkManager>,
        transfer: Arc<FileTransferManager>,
        app_handle: AppHandle,
    ) -> Self {
        Self {
            device_id,
            watcher,
            discovery: Arc::new(std::sync::Mutex::new(discovery)),
            network,
            transfer,
            sync_enabled: Arc::new(AtomicBool::new(true)),
            stats: Arc::new(parking_lot::Mutex::new(SyncStats::default())),
            history: Arc::new(parking_lot::Mutex::new(Vec::new())),
            image_assemblers: Arc::new(parking_lot::Mutex::new(Vec::new())),
            pending_clipboard: Arc::new(parking_lot::Mutex::new(None)),
            app_handle,
        }
    }

    /// 启动同步引擎主循环
    pub async fn run(
        &self,
        mut local_clipboard_rx: mpsc::UnboundedReceiver<ClipboardContent>,
        mut network_rx: mpsc::UnboundedReceiver<NetworkEvent>,
        mut discovery_rx: tokio::sync::broadcast::Receiver<DiscoveryEvent>,
    ) {
        loop {
            tokio::select! {
                // 本地剪贴板变化 → 广播
                Some(content) = local_clipboard_rx.recv() => {
                    if self.sync_enabled.load(Ordering::Relaxed) {
                        self.handle_local_clipboard_change(content).await;
                    }
                }
                // 收到远端消息 → 写入本地
                Some(event) = network_rx.recv() => {
                    self.handle_network_event(event).await;
                }
                // 设备发现事件
                result = discovery_rx.recv() => match result {
                    Ok(event) => {
                        self.handle_discovery_event(event).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("设备发现事件滞后 {} 条，继续运行", n);
                        // Lagged 后可恢复，继续循环
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    }

    async fn handle_local_clipboard_change(&self, content: ClipboardContent) {
        // 记录到历史
        match &content {
            ClipboardContent::Text(t) => {
                self.add_history_entry("text", t, "本机");
            }
            ClipboardContent::Image { data, .. } => {
                let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
                self.add_history_entry("image", &b64, "本机");
            }
            ClipboardContent::None => {}
        }

        // 无连接时缓存内容，设备连上后立即广播（带时间戳）
        if self.network.connected_count() == 0 {
            let hash = content.content_hash();
            *self.pending_clipboard.lock() = Some((content, hash, std::time::Instant::now()));
            return;
        }

        let is_text = matches!(&content, ClipboardContent::Text(_));
        let is_image = matches!(&content, ClipboardContent::Image { .. });

        let msg = match content {
            ClipboardContent::Text(text) => Message::ClipboardText(ClipboardTextPayload {
                source_device_id: self.device_id.clone(),
                content: text,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            }),
            ClipboardContent::Image {
                width,
                height,
                data,
            } => {
                if data.len() < LARGE_IMAGE_THRESHOLD {
                    Message::ClipboardImage(ClipboardImagePayload {
                        source_device_id: self.device_id.clone(),
                        width,
                        height,
                        format: ImageFormat::Png,
                        data,
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    })
                } else {
                    self.send_image_in_chunks(width, height, &data).await;
                    return;
                }
            }
            ClipboardContent::None => return,
        };

        // 广播成功后递增统计
        match self.network.broadcast(&msg) {
            Ok(()) => {
                if is_text {
                    self.stats.lock().texts_synced += 1;
                } else if is_image {
                    self.stats.lock().images_synced += 1;
                }
            }
            Err(e) => {
                tracing::error!("广播剪贴板内容失败: {}", e);
            }
        }
    }

    async fn send_image_in_chunks(&self, width: u32, height: u32, data: &[u8]) {
        self.stats.lock().images_synced += 1;
        let transfer_id = uuid::Uuid::new_v4().to_string();
        let total_chunks = (data.len() + IMAGE_CHUNK_SIZE - 1) / IMAGE_CHUNK_SIZE;
        let total_chunks = total_chunks.min(u16::MAX as usize) as u16;

        for i in 0..total_chunks {
            let start = i as usize * IMAGE_CHUNK_SIZE;
            let end = std::cmp::min(start + IMAGE_CHUNK_SIZE, data.len());
            let chunk = data[start..end].to_vec();

            let msg = Message::ClipboardImageChunk(ClipboardImageChunkPayload {
                source_device_id: self.device_id.clone(),
                transfer_id: transfer_id.clone(),
                width,
                height,
                total_chunks,
                chunk_index: i,
                data: chunk,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });

            if let Err(e) = self.network.broadcast(&msg) {
                tracing::error!("广播图片块 {}/{} 失败: {}", i, total_chunks, e);
            }
        }
    }

    async fn handle_network_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived {
                from_device_id: ref src_id,
                message,
            } => match message {
                Message::ClipboardText(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    // 记录到历史
                    self.add_history_entry("text", &payload.content, src_id);
                    let content = ClipboardContent::Text(payload.content);
                    if let Err(e) = self.watcher.write_safely(&content) {
                        tracing::error!("写入远端文本到剪贴板失败: {}", e);
                    }
                    let _ = self.app_handle.emit("clipboard-updated", &serde_json::json!({"type": "text"}));
                }
                Message::ClipboardImage(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    // 记录到历史
                    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &payload.data);
                    self.add_history_entry("image", &b64, src_id);
                    let content = ClipboardContent::Image {
                        width: payload.width,
                        height: payload.height,
                        data: payload.data,
                    };
                    if let Err(e) = self.watcher.write_safely(&content) {
                        tracing::error!("写入远端图片到剪贴板失败: {}", e);
                    }
                    let _ = self.app_handle.emit("clipboard-updated", &serde_json::json!({"type": "image"}));
                }
                Message::ClipboardImageChunk(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    self.handle_image_chunk(payload);
                }
                Message::FileTransferReq(payload) => {
                    self.transfer.handle_file_request(&payload);
                }
                Message::FileDataChunk(payload) => {
                    if let Err(e) = self.transfer.handle_file_chunk(&payload) {
                        tracing::error!("文件数据块处理失败: {}", e);
                    }
                }
                _ => {}
            },
            NetworkEvent::DeviceConnected { device_name, device_id, platform } => {
                tracing::info!("设备已连接: {} ({})", device_name, device_id);
                if let Err(e) = self.app_handle.emit("device-online", &serde_json::json!({
                    "device_id": device_id,
                    "device_name": device_name,
                    "platform": platform,
                })) {
                    tracing::error!("发送 device-online 事件失败: {}", e);
                }
                // 立即广播缓存的剪贴板内容
                self.flush_pending_clipboard();
            }
            NetworkEvent::DeviceDisconnected { device_id } => {
                tracing::info!("设备已断开: {}", device_id);
                if let Err(e) = self.app_handle.emit("device-offline", &device_id) {
                    tracing::error!("发送 device-offline 事件失败: {}", e);
                }
            }
        }
    }

    fn handle_image_chunk(&self, payload: ClipboardImageChunkPayload) {
        let mut assemblers = self.image_assemblers.lock();

        // 查找或创建组装器
        let assembler = assemblers.iter_mut().find(|a| a.transfer_id == payload.transfer_id);

        if let Some(assembler) = assembler {
            assembler.add_chunk(payload.chunk_index, payload.data);

            if assembler.is_complete() {
                // 取出组装好的内容
                let transfer_id = assembler.transfer_id.clone();
                let content = assembler
                    .chunks
                    .iter()
                    .flatten()
                    .flatten()
                    .cloned()
                    .collect::<Vec<u8>>();

                // 记录到历史
                let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &content);
                self.add_history_entry("image", &b64, &payload.source_device_id);

                let clipboard_content = ClipboardContent::Image {
                    width: assembler.width,
                    height: assembler.height,
                    data: content,
                };

                if let Err(e) = self.watcher.write_safely(&clipboard_content) {
                    tracing::error!("写入远端图片块(已有组装器)到剪贴板失败: {}", e);
                }
                let _ = self.app_handle.emit("clipboard-updated", &serde_json::json!({"type": "image"}));

                // 移除组装器
                assemblers.retain(|a| a.transfer_id != transfer_id);
            }
        } else {
            // 新分块传输
            let mut assembler = ImageChunkAssembler::new(
                payload.transfer_id.clone(),
                payload.width,
                payload.height,
                payload.total_chunks,
            );
            assembler.add_chunk(payload.chunk_index, payload.data);

            if assembler.is_complete() {
                let content = assembler.assemble();
                // 记录到历史
                if let ClipboardContent::Image { data, .. } = &content {
                    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
                    self.add_history_entry("image", &b64, &payload.source_device_id);
                }
                if let Err(e) = self.watcher.write_safely(&content) {
                    tracing::error!("写入远端图片块(新组装器)到剪贴板失败: {}", e);
                }
                let _ = self.app_handle.emit("clipboard-updated", &serde_json::json!({"type": "image"}));
            } else {
                assemblers.push(assembler);
            }
        }
    }

    async fn handle_discovery_event(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::DeviceFound(device) => {
                let device_name = device.device_name.clone();
                match self.network.connect_to_device(&device).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::warn!("连接设备 {} 失败: {}", device_name, e);
                    }
                }
            }
            DiscoveryEvent::DeviceLost(_device_id) => {
                // 连接管理器会自动处理断开
            }
        }
    }

    // ── 内部辅助 ──────────────────────────

    /// 设备首次连接时，广播缓存的剪贴板内容（超过 60 秒则丢弃）
    fn flush_pending_clipboard(&self) {
        let pending = self.pending_clipboard.lock().take();
        if let Some((content, _hash, cached_at)) = pending {
            // 超过 60 秒的缓存视为过时，丢弃
            if cached_at.elapsed() > std::time::Duration::from_secs(60) {
                tracing::debug!("剪贴板缓存已过期，丢弃");
                return;
            }
            tracing::info!("设备已连接，广播缓存的剪贴板内容");
            let msg = match content {
                ClipboardContent::Text(text) => Message::ClipboardText(ClipboardTextPayload {
                    source_device_id: self.device_id.clone(),
                    content: text,
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                }),
                ClipboardContent::Image { width, height, data } => {
                    if data.len() < LARGE_IMAGE_THRESHOLD {
                        Message::ClipboardImage(ClipboardImagePayload {
                            source_device_id: self.device_id.clone(),
                            width,
                            height,
                            format: ImageFormat::Png,
                            data,
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        })
                    } else {
                        // 大图片走分块发送
                        tokio::spawn({
                            let engine_network = self.network.clone();
                            let engine_device_id = self.device_id.clone();
                            async move {
                                let transfer_id = uuid::Uuid::new_v4().to_string();
                                let total_chunks =
                                    ((data.len() + IMAGE_CHUNK_SIZE - 1) / IMAGE_CHUNK_SIZE)
                                        .min(u16::MAX as usize) as u16;
                                for i in 0..total_chunks {
                                    let start = i as usize * IMAGE_CHUNK_SIZE;
                                    let end = std::cmp::min(start + IMAGE_CHUNK_SIZE, data.len());
                                    let chunk = data[start..end].to_vec();
                                    let msg = Message::ClipboardImageChunk(ClipboardImageChunkPayload {
                                        source_device_id: engine_device_id.clone(),
                                        transfer_id: transfer_id.clone(),
                                        width,
                                        height,
                                        total_chunks,
                                        chunk_index: i,
                                        data: chunk,
                                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                    });
                                    if let Err(e) = engine_network.broadcast(&msg) {
                                        tracing::error!(
                                            "广播缓存图片块 {}/{} 失败: {}",
                                            i, total_chunks, e
                                        );
                                    }
                                }
                            }
                        });
                        return;
                    }
                }
                ClipboardContent::None => return,
            };
            if let Err(e) = self.network.broadcast(&msg) {
                tracing::error!("广播缓存的剪贴板内容失败: {}", e);
            }
        }
    }

    // ── 控制接口 ──────────────────────────
    pub fn toggle_sync(&self) -> bool {
        let new_state = !self.sync_enabled.load(Ordering::Relaxed);
        self.sync_enabled.store(new_state, Ordering::Relaxed);
        new_state
    }

    pub fn is_sync_enabled(&self) -> bool {
        self.sync_enabled.load(Ordering::Relaxed)
    }

    /// 更新本机设备名并重新注册 mDNS
    pub fn update_device_name(&self, name: &str) {
        if let Ok(mut discovery) = self.discovery.lock() {
            if let Err(e) = discovery.stop() {
                tracing::warn!("取消旧 mDNS 注册失败: {}", e);
            }
            discovery.set_device_name(name.to_string());
            if let Err(e) = discovery.start() {
                tracing::warn!("mDNS 重新注册失败: {}", e);
            } else {
                tracing::info!("设备名已更新并重新注册 mDNS: {}", name);
            }
        }
    }

    pub fn get_stats(&self) -> SyncStats {
        self.stats.lock().clone()
    }

    pub fn get_history(&self) -> Vec<ClipboardHistoryEntry> {
        self.history.lock().clone()
    }

    pub fn get_discovered_devices(&self) -> Vec<crate::discovery::DiscoveredDevice> {
        self.discovery.lock().unwrap().list_devices()
    }

    pub fn write_to_clipboard(&self, content: &ClipboardContent) -> crate::error::AppResult<()> {
        self.watcher.write_safely(content)
    }

    fn add_history_entry(&self, entry_type: &str, content: &str, from_device: &str) {
        let mut history = self.history.lock();
        let entry = ClipboardHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            entry_type: entry_type.to_string(),
            content: content.to_string(),
            from_device: from_device.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };
        history.insert(0, entry);
        if history.len() > MAX_HISTORY_ENTRIES {
            history.truncate(MAX_HISTORY_ENTRIES);
        }
    }
}
