use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::clipboard::{ClipboardContent, ClipboardWatcher};
use crate::discovery::{DiscoveryEvent, DiscoveryService};
use crate::network::{NetworkEvent, NetworkManager};
use crate::protocol::{ClipboardImageChunkPayload, ClipboardImagePayload, ClipboardTextPayload, ImageFormat, Message};

pub const LARGE_IMAGE_THRESHOLD: usize = 10 * 1024 * 1024; // 10MB
pub const IMAGE_CHUNK_SIZE: usize = 512 * 1024; // 512KB

// ── 同步统计 ──────────────────────────────
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncStats {
    pub texts_synced: u64,
    pub images_synced: u64,
    pub files_transferred: u64,
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
    sync_enabled: Arc<AtomicBool>,
    stats: Arc<parking_lot::Mutex<SyncStats>>,
    image_assemblers: Arc<parking_lot::Mutex<Vec<ImageChunkAssembler>>>,
}

impl SyncEngine {
    pub fn new(
        device_id: String,
        watcher: Arc<ClipboardWatcher>,
        discovery: DiscoveryService,
        network: Arc<NetworkManager>,
    ) -> Self {
        Self {
            device_id,
            watcher,
            discovery: Arc::new(std::sync::Mutex::new(discovery)),
            network,
            sync_enabled: Arc::new(AtomicBool::new(true)),
            stats: Arc::new(parking_lot::Mutex::new(SyncStats::default())),
            image_assemblers: Arc::new(parking_lot::Mutex::new(Vec::new())),
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
                Ok(event) = discovery_rx.recv() => {
                    self.handle_discovery_event(event).await;
                }
                else => break,
            }
        }
    }

    async fn handle_local_clipboard_change(&self, content: ClipboardContent) {
        match &content {
            ClipboardContent::Text(_) => {
                self.stats.lock().texts_synced += 1;
            }
            ClipboardContent::Image { .. } => {
                self.stats.lock().images_synced += 1;
            }
            ClipboardContent::None => return,
        }

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

        let _ = self.network.broadcast(&msg);
    }

    async fn send_image_in_chunks(&self, width: u32, height: u32, data: &[u8]) {
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

            let _ = self.network.broadcast(&msg);
        }
    }

    async fn handle_network_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived {
                from_device_id,
                message,
            } => match message {
                Message::ClipboardText(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    let content = ClipboardContent::Text(payload.content);
                    let _ = self.watcher.write_safely(&content);
                }
                Message::ClipboardImage(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    let content = ClipboardContent::Image {
                        width: payload.width,
                        height: payload.height,
                        data: payload.data,
                    };
                    let _ = self.watcher.write_safely(&content);
                }
                Message::ClipboardImageChunk(payload) => {
                    if payload.source_device_id == self.device_id {
                        return;
                    }
                    self.handle_image_chunk(payload);
                }
                _ => {}
            },
            NetworkEvent::DeviceConnected { device_id } => {
                tracing::info!("设备已连接: {}", device_id);
            }
            NetworkEvent::DeviceDisconnected { device_id } => {
                tracing::info!("设备已断开: {}", device_id);
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

                let clipboard_content = ClipboardContent::Image {
                    width: assembler.width,
                    height: assembler.height,
                    data: content,
                };

                let _ = self.watcher.write_safely(&clipboard_content);

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
                let _ = self.watcher.write_safely(&content);
            } else {
                assemblers.push(assembler);
            }
        }
    }

    async fn handle_discovery_event(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::DeviceFound(device) => {
                let _ = self.network.connect_to_device(&device).await;
            }
            DiscoveryEvent::DeviceLost(_device_id) => {
                // 连接管理器会自动处理断开
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

    pub fn get_stats(&self) -> SyncStats {
        self.stats.lock().clone()
    }
}
