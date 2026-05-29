use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::discovery::DiscoveredDevice;
use crate::error::{AppError, AppResult};
use crate::protocol::{FrameDecoder, HeartbeatPayload, Message};

// ── 心跳常量 ──────────────────────────────
pub const HEARTBEAT_INTERVAL_SECS: u64 = 10;
pub const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

// ── 网络事件 ──────────────────────────────
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    MessageReceived {
        from_device_id: String,
        message: Message,
    },
    DeviceConnected {
        device_name: String,
        device_id: String,
        platform: String,
    },
    DeviceDisconnected {
        device_id: String,
    },
}

// ── 网络管理器 ──────────────────────────────
pub struct NetworkManager {
    device_id: String,
    device_name: String,
    port: u16,
    connections: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
    last_activity: Arc<DashMap<String, Instant>>,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    running: Arc<AtomicBool>,
    #[allow(dead_code)]
    discovery_devices: Arc<DashMap<String, DiscoveredDevice>>,
}

impl NetworkManager {
    pub fn new(
        device_id: String,
        device_name: String,
        port: u16,
        event_tx: mpsc::UnboundedSender<NetworkEvent>,
    ) -> Self {
        Self {
            device_id,
            device_name,
            port,
            connections: Arc::new(DashMap::new()),
            last_activity: Arc::new(DashMap::new()),
            event_tx,
            running: Arc::new(AtomicBool::new(true)),
            discovery_devices: Arc::new(DashMap::new()),
        }
    }

    /// 设置设备发现缓存（用于握手时获取完整设备信息）
    /// 使用 UnsafeCell 实现内部可变性
    pub fn set_discovery_cache(&self, _devices: Arc<DashMap<String, DiscoveredDevice>>) {
        // discovery_devices 暂时未使用，保留接口供后续扩展
    }

    /// 启动 TCP 服务器
    pub async fn start(&self) -> AppResult<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .map_err(|e| {
                AppError::Network(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("端口 {} 已被占用: {}", self.port, e),
                ))
            })?;

        tracing::info!("TCP 服务器已启动: 0.0.0.0:{}", self.port);

        let connections = self.connections.clone();
        let event_tx = self.event_tx.clone();
        let device_id = self.device_id.clone();
        let last_activity = self.last_activity.clone();
        let running = self.running.clone();

        // 心跳 Ping 发送任务
        let hb_conns = self.connections.clone();
        let hb_device_id = self.device_id.clone();
        let hb_running = self.running.clone();
        tokio::spawn(async move {
            while hb_running.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                let ping = Message::HeartbeatPing(HeartbeatPayload {
                    device_id: hb_device_id.clone(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
                let Ok(encoded) = ping.encode() else { continue };
                for entry in hb_conns.iter() {
                    let _ = entry.value().send(encoded.clone());
                }
            }
        });

        // 心跳超时检测任务
        let timeout_conns = self.connections.clone();
        let timeout_activity = self.last_activity.clone();
        let timeout_event = self.event_tx.clone();
        let timeout_running = self.running.clone();
        tokio::spawn(async move {
            while timeout_running.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                let now = Instant::now();
                let timeout = std::time::Duration::from_secs(HEARTBEAT_TIMEOUT_SECS);
                let timed_out: Vec<String> = timeout_activity
                    .iter()
                    .filter(|e| now - *e.value() > timeout)
                    .map(|e| e.key().clone())
                    .collect();
                for id in timed_out {
                    tracing::warn!("设备心跳超时，断开: {}", id);
                    timeout_conns.remove(&id);
                    timeout_activity.remove(&id);
                    let _ = timeout_event.send(NetworkEvent::DeviceDisconnected {
                        device_id: id,
                    });
                }
            }
        });

        tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        tracing::info!("新连接来自: {}", addr);
                        let conns = connections.clone();
                        let evt = event_tx.clone();
                        let did = device_id.clone();
                        let act = last_activity.clone();

                        tokio::spawn(async move {
                            if let Err(e) =
                                Self::handle_connection(stream, did, conns, evt, act).await
                            {
                                tracing::error!("连接处理错误: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        if running.load(Ordering::Relaxed) {
                            tracing::error!("接受连接失败: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn handle_connection(
        stream: TcpStream,
        _my_device_id: String,
        connections: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
        event_tx: mpsc::UnboundedSender<NetworkEvent>,
        last_activity: Arc<DashMap<String, Instant>>,
    ) -> AppResult<()> {
        // 等待握手消息
        let mut decoder = FrameDecoder::new();
        let mut buf = vec![0u8; 65536];

        // 尝试读取
        loop {
            match stream.try_read(&mut buf) {
                Ok(0) => return Err(AppError::Protocol("连接关闭".into())),
                Ok(n) => {
                    let messages = decoder.feed(&buf[..n]);
                    if let Some(device_info) = messages.iter().find_map(|m| {
                        if let Message::DeviceInfo(payload) = m {
                            Some((payload.device_id.clone(), payload.device_name.clone(), payload.platform.clone()))
                        } else {
                            None
                        }
                    }) {
                        let (remote_device_id, remote_device_name, remote_platform) = device_info;
                        // 防止自我连接
                        if remote_device_id == _my_device_id {
                            tracing::warn!("检测到自我连接，忽略");
                            return Ok(());
                        }
                        // 防止重复连接：双向发现时双方各自发起连接，只保留第一条
                        if connections.contains_key(&remote_device_id) {
                            tracing::debug!(
                                "设备 {} 已连接，忽略重复连接请求",
                                remote_device_name
                            );
                            return Ok(());
                        }
                        tracing::info!("设备握手完成: {} ({})", remote_device_name, remote_device_id);

                        let (send_tx, mut send_rx) =
                            tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

                        connections.insert(remote_device_id.clone(), send_tx);
                        last_activity.insert(remote_device_id.clone(), Instant::now());

                        let _ = event_tx.send(NetworkEvent::DeviceConnected {
                            device_name: remote_device_name,
                            device_id: remote_device_id.clone(),
                            platform: remote_platform,
                        });

                        // 启动写入任务
                        let (read_half, mut write_half) = stream.into_split();

                        let write_handle = {
                            let remote_id = remote_device_id.clone();
                            tokio::spawn(async move {
                                while let Some(data) = send_rx.recv().await {
                                    if let Err(e) = write_half.write_all(&data).await {
                                        tracing::error!("写入 {} 失败: {}", remote_id, e);
                                        break;
                                    }
                                }
                            })
                        };

                        // 读取循环
                        let mut decoder2 = FrameDecoder::new();
                        let mut buf2 = vec![0u8; 65536];
                        let act = last_activity.clone();
                        let send_for_pong = connections.clone();
                        let remote_rid = remote_device_id.clone();
                        let my_id = _my_device_id.clone();

                        loop {
                            match read_half.try_read(&mut buf2) {
                                Ok(0) => break,
                                Ok(n) => {
                                    let msgs = decoder2.feed(&buf2[..n]);
                                    for msg in msgs {
                                        match &msg {
                                            Message::HeartbeatPing(_) | Message::HeartbeatPong(_) => {
                                                act.insert(remote_rid.clone(), Instant::now());
                                            }
                                            _ => {}
                                        }
                                        // 收到 Ping 自动回复 Pong
                                        if matches!(&msg, Message::HeartbeatPing(_)) {
                                            let pong = Message::HeartbeatPong(HeartbeatPayload {
                                                device_id: my_id.clone(),
                                                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                            });
                                            if let Ok(encoded) = pong.encode() {
                                                if let Some(tx) = send_for_pong.get(&remote_rid) {
                                                    let _ = tx.send(encoded);
                                                }
                                            }
                                            continue; // Ping 不需要转发到 SyncEngine
                                        }
                                        let _ = event_tx.send(NetworkEvent::MessageReceived {
                                            from_device_id: remote_device_id.clone(),
                                            message: msg,
                                        });
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100))
                                        .await;
                                    continue;
                                }
                                Err(_) => break,
                            }
                        }

                        // 断开
                        connections.remove(&remote_device_id);
                        last_activity.remove(&remote_device_id);
                        let _ = event_tx.send(NetworkEvent::DeviceDisconnected {
                            device_id: remote_device_id.clone(),
                        });
                        write_handle.abort();

                        tracing::info!("设备断开: {}", remote_device_id);
                        return Ok(());
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
                Err(e) => return Err(AppError::Network(e)),
            }
        }
    }

    /// 主动连接一个发现的设备
    pub async fn connect_to_device(&self, device: &DiscoveredDevice) -> AppResult<()> {
        // 防止连接到自身或空设备
        if device.device_id == self.device_id || device.device_id.is_empty() {
            tracing::debug!("跳过自身连接: device_id={}", device.device_id);
            return Ok(());
        }
        if self.connections.contains_key(&device.device_id) {
            return Ok(());
        }

        let addr = format!("{}:{}", device.ip_address, device.tcp_port);
        tracing::info!("正在连接设备: {} ({})...", device.device_name, addr);

        let mut stream = TcpStream::connect(&addr).await.map_err(|e| {
            AppError::Network(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("连接 {} 失败: {}", addr, e),
            ))
        })?;

        // 发送握手消息
        let handshake = Message::DeviceInfo(crate::protocol::DeviceInfoPayload {
            device_id: self.device_id.clone(),
            device_name: self.device_name.clone(),
            hostname: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            platform: std::env::consts::OS.to_string(),
            tcp_port: self.port,
            version: env!("CARGO_PKG_VERSION").to_string(),
        });

        let encoded = handshake.encode()?;
        stream.write_all(&encoded).await?;

        let (send_tx, mut send_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        self.connections
            .insert(device.device_id.clone(), send_tx);
        self.last_activity.insert(device.device_id.clone(), Instant::now());

        let _ = self.event_tx.send(NetworkEvent::DeviceConnected {
            device_name: device.device_name.clone(),
            device_id: device.device_id.clone(),
            platform: device.platform.clone(),
        });

        // 后台任务处理读写
        let (read_half, mut write_half) = stream.into_split();
        let conns = self.connections.clone();
        let evt = self.event_tx.clone();
        let remote_id = device.device_id.clone();
        let act = self.last_activity.clone();
        let my_id = self.device_id.clone();

        tokio::spawn(async move {
            // 写入任务
            let write_task = tokio::spawn(async move {
                while let Some(data) = send_rx.recv().await {
                    if write_half.write_all(&data).await.is_err() {
                        break;
                    }
                }
            });

            // 读取
            let mut decoder = FrameDecoder::new();
            let mut buf = vec![0u8; 65536];
            loop {
                match read_half.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for msg in decoder.feed(&buf[..n]) {
                            match &msg {
                                Message::HeartbeatPing(_) | Message::HeartbeatPong(_) => {
                                    act.insert(remote_id.clone(), Instant::now());
                                }
                                _ => {}
                            }
                            if matches!(&msg, Message::HeartbeatPing(_)) {
                                let pong = Message::HeartbeatPong(HeartbeatPayload {
                                    device_id: my_id.clone(),
                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                });
                                if let Ok(encoded) = pong.encode() {
                                    if let Some(tx) = conns.get(&remote_id) {
                                        let _ = tx.send(encoded);
                                    }
                                }
                                continue;
                            }
                            let _ = evt.send(NetworkEvent::MessageReceived {
                                from_device_id: remote_id.clone(),
                                message: msg,
                            });
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                    Err(_) => break,
                }
            }

            write_task.abort();
            conns.remove(&remote_id);
            act.remove(&remote_id);
            let _ = evt.send(NetworkEvent::DeviceDisconnected {
                device_id: remote_id,
            });
        });

        Ok(())
    }

    /// 发送消息到指定设备
    pub fn send(&self, target_device_id: &str, message: &Message) -> AppResult<()> {
        let tx = self
            .connections
            .get(target_device_id)
            .ok_or_else(|| AppError::Network(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                format!("设备未连接: {}", target_device_id),
            )))?;
        let encoded = message.encode()?;
        tx.send(encoded).map_err(|_| AppError::ChannelSend)
    }

    /// 广播消息到所有已连接设备
    pub fn broadcast(&self, message: &Message) -> AppResult<()> {
        let encoded = message.encode()?;
        for entry in self.connections.iter() {
            let _ = entry.value().send(encoded.clone());
        }
        Ok(())
    }

    /// 获取已连接设备数
    pub fn connected_count(&self) -> usize {
        self.connections.len()
    }

    /// 停止所有网络活动
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
