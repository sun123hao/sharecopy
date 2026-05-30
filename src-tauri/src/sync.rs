use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::clipboard::{ClipboardContent, ClipboardWatcher};
use crate::discovery::{DiscoveryEvent, DiscoveryService};
use crate::error::AppError;
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
    created_at: Instant,
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
            created_at: Instant::now(),
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
    /// 重连退避追踪：记录每个设备的上次重连时间
    reconnect_backoff: Arc<dashmap::DashMap<String, (Instant, u32)>>, // (上次重连时间, 当前退避级别)
    /// TCP 连接过的设备信息缓存（mDNS 不可达时用于 UI 显示）
    connected_info: Arc<dashmap::DashMap<String, crate::discovery::DiscoveredDevice>>,
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
            reconnect_backoff: Arc::new(dashmap::DashMap::new()),
            connected_info: Arc::new(dashmap::DashMap::new()),
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
        // 定期重连 + 清理定时器（15s，首次 tick 加少量随机延迟避免雷同）
        let jitter_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos()
            % 5000) as u64;
        let mut reconnect_timer = tokio::time::interval(
            tokio::time::Duration::from_secs(15) + tokio::time::Duration::from_millis(jitter_ms),
        );

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
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                // 定期重连检查 + 清理
                _ = reconnect_timer.tick() => {
                    self.periodic_reconnect_and_cleanup();
                },
            }
        }
    }

    async fn handle_local_clipboard_change(&self, content: ClipboardContent) {
        // ClipboardContent::None 无需处理
        if matches!(&content, ClipboardContent::None) {
            return;
        }

        // 记录到历史
        match &content {
            ClipboardContent::Text(t) => {
                self.add_history_entry("text", t, "本机");
            }
            ClipboardContent::Image { data, .. } => {
                let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
                self.add_history_entry("image", &b64, "本机");
            }
            ClipboardContent::None => unreachable!(),
        }

        // 无连接时缓存内容，设备连上后立即广播（带时间戳）
        if self.network.connected_count() == 0 {
            let hash = content.content_hash();
            *self.pending_clipboard.lock() = Some((content, hash, std::time::Instant::now()));
            tracing::debug!("无连接设备，剪贴板内容已缓存");
            return;
        }

        // 提前计算 hash（content 后续会被 match 消耗/部分移动）
        let content_hash = content.content_hash();
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
            ClipboardContent::None => unreachable!(),
        };

        // 广播，无连接时回退到缓存路径
        match self.network.broadcast(&msg) {
            Ok(()) => {
                if is_text {
                    self.stats.lock().texts_synced += 1;
                } else if is_image {
                    self.stats.lock().images_synced += 1;
                }
            }
            Err(AppError::NoConnection) => {
                // 广播时刚好连接断开，从原始消息重建内容以缓存
                tracing::debug!("广播时无连接，尝试从消息重建缓存");
                let cached = Self::content_from_message(&msg);
                if let Some(c) = cached {
                    *self.pending_clipboard.lock() = Some((c, content_hash, std::time::Instant::now()));
                }
            }
            Err(e) => {
                tracing::error!("广播剪贴板内容失败: {}", e);
            }
        }
    }

    /// 从 Message 反向重建 ClipboardContent（仅用于缓存回退）
    fn content_from_message(msg: &Message) -> Option<ClipboardContent> {
        match msg {
            Message::ClipboardText(p) => Some(ClipboardContent::Text(p.content.clone())),
            Message::ClipboardImage(p) => Some(ClipboardContent::Image {
                width: p.width,
                height: p.height,
                data: p.data.clone(),
            }),
            _ => None,
        }
    }

    async fn send_image_in_chunks(&self, width: u32, height: u32, data: &[u8]) {
        // 无连接时直接返回，不递增统计
        if self.network.connected_count() == 0 {
            tracing::debug!("无连接设备，跳过大图片分块发送");
            return;
        }

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let total_chunks = (data.len() + IMAGE_CHUNK_SIZE - 1) / IMAGE_CHUNK_SIZE;
        let total_chunks = total_chunks.min(u16::MAX as usize) as u16;

        let mut all_success = true;
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

            match self.network.broadcast(&msg) {
                Ok(()) => {} // 成功，继续
                Err(AppError::NoConnection) => {
                    tracing::debug!("大图片分块广播时无连接，中断发送");
                    all_success = false;
                    break;
                }
                Err(e) => {
                    tracing::error!("广播图片块 {}/{} 失败: {}", i, total_chunks, e);
                    all_success = false;
                    break;
                }
            }
        }

        // 所有 chunk 都成功才递增统计
        if all_success {
            self.stats.lock().images_synced += 1;
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
                // 缓存设备信息（mDNS 不可达时 UI 也能显示）
                self.connected_info.insert(device_id.clone(), crate::discovery::DiscoveredDevice {
                    device_id: device_id.clone(),
                    device_name: device_name.clone(),
                    hostname: String::new(),
                    platform: platform.clone(),
                    ip_address: String::new(),
                    tcp_port: 0,
                    last_seen: chrono::Utc::now(),
                    first_seen: chrono::Utc::now(),
                });
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
                // 如果设备仍在 mDNS 发现列表中，尝试重连
                self.try_reconnect_device(&device_id);
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
                // mDNS 刷新时清除退避，准备快速重连
                self.reconnect_backoff.remove(&device.device_id);
                let is_android = std::env::consts::OS == "android";
                let remote_is_android = device.platform == "android";

                let should_connect = if is_android {
                    // Android 始终主动连接（作为 server 因 WiFi 休眠不可靠）
                    true
                } else if remote_is_android {
                    // 对方是 Android → 让它主动连，本端不连（避免双向）
                    false
                } else {
                    // 非 Android 之间：小端连大端
                    device.device_id > self.device_id
                };

                if should_connect {
                    let device_name = device.device_name.clone();
                    match self.network.connect_to_device(&device).await {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::warn!("连接设备 {} 失败: {}", device_name, e);
                        }
                    }
                } else {
                    tracing::debug!(
                        "设备 {} 等待对方主动连接（30s 超时后回退）",
                        device.device_name
                    );
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
        if let Some((content, hash, cached_at)) = pending {
            // 超过 60 秒的缓存视为过时，丢弃
            if cached_at.elapsed() > std::time::Duration::from_secs(60) {
                tracing::debug!("剪贴板缓存已过期，丢弃");
                return;
            }
            tracing::info!("设备已连接，广播缓存的剪贴板内容");
            // 克隆一份用于 NoConnection 时重新缓存
            let content_clone = content.clone();
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
                        // 大图片分块发送：先重新缓存（防止广播失败丢失内容）
                        *self.pending_clipboard.lock() = Some((
                            ClipboardContent::Image {
                                width,
                                height,
                                data: data.clone(),
                            },
                            hash.clone(),
                            std::time::Instant::now(),
                        ));
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
                                        if matches!(&e, AppError::NoConnection) {
                                            tracing::debug!("缓存图片分块广播时无连接，中断");
                                            break;
                                        }
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
            match self.network.broadcast(&msg) {
                Ok(()) => {}
                Err(AppError::NoConnection) => {
                    // 连接已断，用克隆副本重新缓存
                    let hash = content_clone.content_hash();
                    *self.pending_clipboard.lock() =
                        Some((content_clone, hash, std::time::Instant::now()));
                    tracing::debug!("flush 时无连接，重新缓存");
                }
                Err(e) => {
                    tracing::error!("广播缓存的剪贴板内容失败: {}", e);
                }
            }
        }
    }

    // ── 重连逻辑 ──────────────────────────

    /// 尝试重连已发现的设备（带指数退避）
    fn try_reconnect_device(&self, device_id: &str) {
        // Android 始终主动重连；非 Android 遇到 Android 设备时不重连（等 Android 主动）
        let device = {
            if let Ok(discovery) = self.discovery.lock() {
                discovery.list_devices().into_iter().find(|d| d.device_id == device_id)
            } else {
                None
            }
        };
        let Some(ref device) = device else { return };
        let is_android = std::env::consts::OS == "android";
        let remote_is_android = device.platform == "android";

        if !is_android && remote_is_android {
            // 对方是 Android → 等它主动连
            return;
        }

        if !is_android && device_id <= self.device_id.as_str() {
            // 非 Android 非小端等待 30s 回退
            let should_fallback = {
                if let Ok(discovery) = self.discovery.lock() {
                    discovery.list_devices().into_iter().any(|d| {
                        d.device_id == device_id
                            && (chrono::Utc::now() - d.first_seen).num_seconds() > 30
                    })
                } else {
                    false
                }
            };
            if !should_fallback {
                return;
            }
            tracing::info!("设备 {} 等待超时，主动回退连接", device_id);
        }
        // 检查是否已连接（可能已由对端重新连接）
        if self.network.is_connected(device_id) {
            return;
        }

        // 指数退避：根据上次重连时间决定是否允许此次重连
        let now = Instant::now();
        if let Some(entry) = self.reconnect_backoff.get(device_id) {
            let (last_attempt, level) = entry.value().clone();
            let backoff_duration = std::time::Duration::from_secs(
                std::cmp::min(2u64.saturating_pow(level), 60), // 2^level 秒，上限 60s
            );
            if now - last_attempt < backoff_duration {
                tracing::debug!(
                    "设备 {} 重连退避中 (级别 {}, 还需等待)",
                    device_id,
                    level
                );
                return;
            }
        }

        // 执行重连
        let device_clone = device.clone();
        let network = self.network.clone();
        let devices_map = self.reconnect_backoff.clone();
        let device_id_owned = device_id.to_string();

        tokio::spawn(async move {
            tracing::info!("尝试重连设备: {} ({})", device_clone.device_name, device_id_owned);
            match network.connect_to_device(&device_clone).await {
                Ok(()) => {
                    tracing::info!("重连设备成功: {}", device_clone.device_name);
                    // 成功后清除退避记录
                    devices_map.remove(&device_id_owned);
                }
                Err(e) => {
                    tracing::warn!("重连设备失败: {} ({})", device_clone.device_name, e);
                    // 更新退避级别
                    devices_map
                        .entry(device_id_owned)
                        .and_modify(|(last, level)| {
                            *last = Instant::now();
                            *level = std::cmp::min(level.saturating_add(1), 10); // 最高 2^10 = 1024s
                        })
                        .or_insert((Instant::now(), 0));
                }
            }
        });
    }

    /// 定期重连检查 + 清理过期数据
    fn periodic_reconnect_and_cleanup(&self) {
        // 1. 重连：在锁内收集需重连的设备 ID，释放锁后再执行
        let to_reconnect: Vec<String> = {
            if let Ok(discovery) = self.discovery.lock() {
                discovery
                    .list_devices()
                    .into_iter()
                    .filter(|d| d.device_id != self.device_id && !self.network.is_connected(&d.device_id))
                    .map(|d| d.device_id)
                    .collect()
            } else {
                Vec::new()
            }
        };
        for device_id in to_reconnect {
            self.try_reconnect_device(&device_id);
        }

        // 2. 清理过期 mDNS 设备 + 同步清理 connections 残留
        // 与重连用同一个锁，避免二次加锁
        {
            // 获取需要清理的过期设备列表
            let stale_ids: Vec<String> = {
                if let Ok(discovery) = self.discovery.lock() {
                    discovery.clean_stale_devices(300)
                } else {
                    Vec::new()
                }
            };
            for device_id in &stale_ids {
                self.reconnect_backoff.remove(device_id);
            }
            if !stale_ids.is_empty() {
                tracing::info!("清理了 {} 个过期设备", stale_ids.len());
            }
        }

        // 3. 清理超过 60s 未完成的图片组装器（防止内存泄漏）
        {
            let mut assemblers = self.image_assemblers.lock();
            let before = assemblers.len();
            let timeout = std::time::Duration::from_secs(60);
            assemblers.retain(|a| {
                let keep = a.received < a.total_chunks
                    && a.created_at.elapsed() < timeout;
                if !keep && a.received < a.total_chunks {
                    tracing::debug!(
                        "清理超时图片组装器: transfer_id={}, received={}/{}, age={:?}",
                        a.transfer_id,
                        a.received,
                        a.total_chunks,
                        a.created_at.elapsed()
                    );
                }
                keep
            });
            if assemblers.len() != before {
                tracing::debug!("清理了 {} 个陈旧图片组装器", before - assemblers.len());
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
        self.discovery.lock().map(|d| d.list_devices()).unwrap_or_default()
    }

    /// 获取 TCP 已连接设备的缓存信息（mDNS 不可达时用）
    pub fn get_connected_device_info(&self, device_id: &str) -> Option<crate::discovery::DiscoveredDevice> {
        self.connected_info.get(device_id).map(|d| d.clone())
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
