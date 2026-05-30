use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use crate::discovery::DiscoveredDevice;
use crate::error::{AppError, AppResult};
use crate::protocol::{FrameDecoder, HeartbeatPayload, Message};

// ── 心跳常量 ──────────────────────────────
pub const HEARTBEAT_INTERVAL_SECS: u64 = 10;
pub const HEARTBEAT_TIMEOUT_SECS: u64 = 30;
/// TCP 写入超时：如果 5 秒内无法完成写入，认为连接已断开
pub const WRITE_TIMEOUT_SECS: u64 = 5;

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
    /// 已通知前端的设备（防止双向连接导致重复 DeviceConnected）
    notified: Arc<DashMap<String, ()>>,
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
            notified: Arc::new(DashMap::new()),
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
        let notified = self.notified.clone();

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
                    if let Err(e) = entry.value().send(encoded.clone()) {
                        tracing::debug!("心跳 Ping 发送失败 (设备 {} 通道已关闭): {:?}", entry.key(), e);
                    }
                }
            }
        });

        // 心跳超时检测任务
        let timeout_conns = self.connections.clone();
        let timeout_activity = self.last_activity.clone();
        let timeout_event = self.event_tx.clone();
        let timeout_notified = self.notified.clone();
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
                    timeout_notified.remove(&id);
                    if let Err(e) = timeout_event.send(NetworkEvent::DeviceDisconnected {
                        device_id: id,
                    }) {
                        tracing::warn!("发送心跳超时断开事件失败: {:?}", e);
                    }
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
                        let ntfy = notified.clone();

                        tokio::spawn(async move {
                            if let Err(e) =
                                Self::handle_connection(stream, did, conns, evt, act, ntfy).await
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
        mut stream: TcpStream,
        _my_device_id: String,
        connections: Arc<DashMap<String, tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
        event_tx: mpsc::UnboundedSender<NetworkEvent>,
        last_activity: Arc<DashMap<String, Instant>>,
        notified: Arc<DashMap<String, ()>>,
    ) -> AppResult<()> {
        // 等待握手消息（用 read().await，完整 TcpStream 无 split 问题）
        let mut decoder = FrameDecoder::new();
        let mut buf = vec![0u8; 65536];

        loop {
            match stream.read(&mut buf).await {
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
                        // 已有连接 → 检查是否为占位符（双方同时发起连接）
                        if connections.contains_key(&remote_device_id) {
                            let is_placeholder = connections
                                .get(&remote_device_id)
                                .map(|s| s.is_closed())
                                .unwrap_or(false);

                            if is_placeholder {
                                // 双方同时发起连接：小端 device_id 接受传入连接
                                // connect_to_device 中的检测会放弃主动连接
                                if _my_device_id < remote_device_id {
                                    tracing::info!(
                                        "双方同时连接 {}，我方胜出，接受传入连接",
                                        remote_device_name
                                    );
                                    connections.remove(&remote_device_id);
                                    // 不 return，继续使用此传入连接
                                } else {
                                    tracing::debug!(
                                        "双方同时连接 {}，对方胜出，保持主动连接",
                                        remote_device_name
                                    );
                                    use tokio::io::AsyncWriteExt;
                                    let _ = stream.shutdown().await;
                                    return Ok(());
                                }
                            } else {
                                // 真正的重复连接（已有活跃连接）→ 优雅关闭
                                tracing::debug!(
                                    "设备 {} 已连接，优雅关闭重复连接",
                                    remote_device_name
                                );
                                use tokio::io::AsyncWriteExt;
                                let _ = stream.shutdown().await;
                                let mut drain_buf = vec![0u8; 4096];
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(1),
                                    async {
                                        loop {
                                            match stream.read(&mut drain_buf).await {
                                                Ok(0) | Err(_) => break,
                                                Ok(_) => {}
                                            }
                                        }
                                    },
                                )
                                .await;
                                return Ok(());
                            }
                        }

                        let (send_tx, mut send_rx) =
                            tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
                        connections.insert(remote_device_id.clone(), send_tx);
                        last_activity.insert(remote_device_id.clone(), Instant::now());

                        tracing::info!("设备握手完成: {} ({})", remote_device_name, remote_device_id);

                        // 仅首次连接时通知前端
                        if notified.insert(remote_device_id.clone(), ()).is_none() {
                            if let Err(e) = event_tx.send(NetworkEvent::DeviceConnected {
                                device_name: remote_device_name,
                                device_id: remote_device_id.clone(),
                                platform: remote_platform,
                            }) {
                                tracing::warn!("发送 DeviceConnected 事件失败: {:?}", e);
                            }
                        }

                        // 启动读写（tokio::io::split 用 Mutex 协调 reactor，read() 安全）
                        let (mut read_half, mut write_half) = tokio::io::split(stream);

                        // oneshot 通道：写任务退出时通知读循环立即清理，避免半死连接
                        let (write_done_tx, mut write_done_rx) =
                            tokio::sync::oneshot::channel::<()>();

                        let write_handle = {
                            let remote_id = remote_device_id.clone();
                            tokio::spawn(async move {
                                while let Some(data) = send_rx.recv().await {
                                    // 写入带超时：TCP 重传超时可能长达 1-2 分钟，
                                    // 用 tokio::time::timeout 在 5 秒内检测到死连接
                                    match tokio::time::timeout(
                                        tokio::time::Duration::from_secs(WRITE_TIMEOUT_SECS),
                                        write_half.write_all(&data),
                                    )
                                    .await
                                    {
                                        Ok(Ok(())) => {} // 写入成功
                                        Ok(Err(e)) => {
                                            tracing::error!("写入 {} 失败: {}", remote_id, e);
                                            break;
                                        }
                                        Err(_elapsed) => {
                                            tracing::warn!(
                                                "写入 {} 超时 ({}s)，连接可能已断开",
                                                remote_id,
                                                WRITE_TIMEOUT_SECS
                                            );
                                            break;
                                        }
                                    }
                                }
                                // 写任务退出时通知读循环（无论原因：超时、错误、通道关闭）
                                let _ = write_done_tx.send(());
                            })
                        };

                        // 读取循环（read().await 由 reactor 唤醒，无轮询延迟）
                        // tokio::select! 同时等待数据到达和写任务退出信号
                        // 复用初始化的 decoder，避免丢失 DeviceInfo 之后的残留字节
                        let mut buf2 = vec![0u8; 65536];
                        let act = last_activity.clone();
                        let send_for_pong = connections.clone();
                        let remote_rid = remote_device_id.clone();
                        let my_id = _my_device_id.clone();

                        loop {
                            tokio::select! {
                                result = read_half.read(&mut buf2) => {
                                    match result {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            let msgs = decoder.feed(&buf2[..n]);
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
                                                            if tx.send(encoded).is_err() {
                                                                tracing::debug!("Pong 发送失败: 通道已关闭");
                                                            }
                                                        }
                                                    }
                                                    continue; // Ping 不需要转发到 SyncEngine
                                                }
                                                if let Err(e) = event_tx.send(NetworkEvent::MessageReceived {
                                                    from_device_id: remote_device_id.clone(),
                                                    message: msg,
                                                }) {
                                                    tracing::warn!("发送 MessageReceived 事件失败: {:?}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("读取 {} 错误: {}", remote_device_id, e);
                                            break;
                                        }
                                    }
                                }
                                _ = &mut write_done_rx => {
                                    // 写任务先于读循环退出 → 连接已半死，立即清理
                                    tracing::warn!(
                                        "写入任务已退出 (超时/错误)，主动断开 {}",
                                        remote_device_id
                                    );
                                    break;
                                }
                            }
                        }

                        // 断开
                        connections.remove(&remote_device_id);
                        last_activity.remove(&remote_device_id);
                        notified.remove(&remote_device_id);
                        if let Err(e) = event_tx.send(NetworkEvent::DeviceDisconnected {
                            device_id: remote_device_id.clone(),
                        }) {
                            tracing::warn!("发送 DeviceDisconnected 事件失败 (handle_connection): {:?}", e);
                        }
                        write_handle.abort();

                        tracing::info!("设备断开: {}", remote_device_id);
                        return Ok(());
                    }
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
        // 原子占位：用 entry API 确保原子 check-and-insert
        let (placeholder_tx, _) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        match self.connections.entry(device.device_id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                tracing::debug!("设备 {} 已被连接，跳过", device.device_name);
                return Ok(());
            }
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                vacant.insert(placeholder_tx);
            }
        }

        let addr = format!("{}:{}", device.ip_address, device.tcp_port);
        tracing::info!("正在连接设备: {} ({})...", device.device_name, addr);

        let mut stream = match TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                self.connections.remove(&device.device_id);
                self.notified.remove(&device.device_id);
                return Err(AppError::Network(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    format!("连接 {} 失败: {}", addr, e),
                )));
            }
        };

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
        if let Err(e) = stream.write_all(&encoded).await {
            self.connections.remove(&device.device_id);
            self.notified.remove(&device.device_id);
            return Err(AppError::Network(e));
        }

        // 检查占位符是否被传入连接替代（双方同时连接时，小端 ID 会接受传入连接）
        if let Some(sender) = self.connections.get(&device.device_id) {
            if !sender.is_closed() {
                // 占位符已被替换为真实 sender → 传入连接已接管
                tracing::info!(
                    "主动连接 {} 已被传入连接接管，放弃",
                    device.device_name
                );
                return Ok(());
            }
        }

        let (send_tx, mut send_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // 替换占位 sender 为真实 sender
        self.connections
            .insert(device.device_id.clone(), send_tx);
        self.last_activity.insert(device.device_id.clone(), Instant::now());

        // 仅首次连接时通知前端（防止双向连接重复通知）
        if self.notified.insert(device.device_id.clone(), ()).is_none() {
            if let Err(e) = self.event_tx.send(NetworkEvent::DeviceConnected {
                device_name: device.device_name.clone(),
                device_id: device.device_id.clone(),
                platform: device.platform.clone(),
            }) {
                tracing::warn!("发送 DeviceConnected 事件失败 (connect): {:?}", e);
            }
        }

        // 后台任务处理读写（tokio::io::split 用 Mutex 协调 reactor，read() 安全）
        let (mut read_half, mut write_half) = tokio::io::split(stream);
        let conns = self.connections.clone();
        let evt = self.event_tx.clone();
        let remote_id = device.device_id.clone();
        let act = self.last_activity.clone();
        let ntfy = self.notified.clone();
        let my_id = self.device_id.clone();

        tokio::spawn(async move {
            // oneshot 通道：写任务退出时通知读循环立即清理
            let (write_done_tx, mut write_done_rx) =
                tokio::sync::oneshot::channel::<()>();

            // 写入任务（需克隆 remote_id 供日志使用）
            let write_remote_id = remote_id.clone();
            let write_task = tokio::spawn(async move {
                while let Some(data) = send_rx.recv().await {
                    // 写入带超时，避免 TCP 重传阻塞整个同步管道
                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(WRITE_TIMEOUT_SECS),
                        write_half.write_all(&data),
                    )
                    .await
                    {
                        Ok(Ok(())) => {} // 写入成功
                        Ok(Err(e)) => {
                            tracing::error!("写入 {} 失败 (connect): {}", write_remote_id, e);
                            break;
                        }
                        Err(_elapsed) => {
                            tracing::warn!(
                                "写入 {} 超时 ({}s, connect)，连接可能已断开",
                                write_remote_id,
                                WRITE_TIMEOUT_SECS
                            );
                            break;
                        }
                    }
                }
                // 写任务退出时通知读循环（无论原因）
                let _ = write_done_tx.send(());
            });

            // 读取（read().await + select 检测写任务退出）
            let mut decoder = FrameDecoder::new();
            let mut buf = vec![0u8; 65536];
            loop {
                tokio::select! {
                    result = read_half.read(&mut buf) => {
                        match result {
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
                                                if tx.send(encoded).is_err() {
                                                    tracing::debug!("Pong 发送失败 (connect): 通道已关闭");
                                                }
                                            }
                                        }
                                        continue;
                                    }
                                    if let Err(e) = evt.send(NetworkEvent::MessageReceived {
                                        from_device_id: remote_id.clone(),
                                        message: msg,
                                    }) {
                                        tracing::warn!("发送 MessageReceived 事件失败 (connect): {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("读取 {} 错误 (connect): {}", remote_id, e);
                                break;
                            }
                        }
                    }
                    _ = &mut write_done_rx => {
                        // 写任务先于读循环退出 → 立即清理
                        tracing::warn!(
                            "写入任务已退出 (connect)，主动断开 {}",
                            remote_id
                        );
                        break;
                    }
                }
            }

            write_task.abort();
            conns.remove(&remote_id);
            act.remove(&remote_id);
            ntfy.remove(&remote_id);
            if let Err(e) = evt.send(NetworkEvent::DeviceDisconnected {
                device_id: remote_id.clone(),
            }) {
                tracing::warn!("发送 DeviceDisconnected 事件失败 (connect): {:?}", e);
            }
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
    /// 至少有一个设备发送成功时返回 Ok，全部失败时返回 Err
    /// 没有已连接设备时返回 NoConnection 错误
    pub fn broadcast(&self, message: &Message) -> AppResult<()> {
        let total = self.connections.len();
        if total == 0 {
            return Err(AppError::NoConnection); // 无连接设备，调用方应缓存或等待
        }
        let encoded = message.encode()?;
        let mut success_count = 0u32;
        for entry in self.connections.iter() {
            match entry.value().send(encoded.clone()) {
                Ok(()) => success_count += 1,
                Err(e) => {
                    tracing::debug!(
                        "广播发送失败 (设备 {} 通道已关闭): {:?}",
                        entry.key(),
                        e
                    );
                }
            }
        }
        if success_count == 0 {
            Err(AppError::Network(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                format!("广播失败: {} 个设备全部无法送达", total),
            )))
        } else {
            Ok(())
        }
    }

    /// 获取已连接的设备 ID 列表
    pub fn connected_device_ids(&self) -> Vec<String> {
        self.connections.iter().map(|e| e.key().clone()).collect()
    }

    /// 检查指定设备是否已连接
    pub fn is_connected(&self, device_id: &str) -> bool {
        self.connections.contains_key(device_id)
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
