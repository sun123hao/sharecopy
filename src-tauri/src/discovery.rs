use dashmap::DashMap;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::error::AppResult;

// ── 发现的设备 ──────────────────────────────
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredDevice {
    pub device_id: String,
    pub device_name: String,
    pub hostname: String,
    pub platform: String,
    pub ip_address: String,
    pub tcp_port: u16,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    /// 首次发现时间（不随 mDNS 刷新更新，用于回退超时判断）
    pub first_seen: chrono::DateTime<chrono::Utc>,
}

// ── 发现事件 ──────────────────────────────
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    DeviceFound(DiscoveredDevice),
    DeviceLost(String), // device_id
}

const SERVICE_TYPE: &str = "_sharecopy._tcp.local.";

// ── 发现服务 ──────────────────────────────
pub struct DiscoveryService {
    mdns: ServiceDaemon,
    service_name: String,
    discovered_devices: Arc<DashMap<String, DiscoveredDevice>>,
    event_tx: broadcast::Sender<DiscoveryEvent>,
    device_id: String,
    device_name: String,
    hostname: String,
    platform: String,
    tcp_port: u16,
    /// 手动指定的本机 IP（Android 上通过 JNI 获取）
    my_ip: Option<std::net::IpAddr>,
    /// 防止重复创建浏览器线程（改名时 re-register 会再次调用 start）
    browser_started: bool,
    /// 上次重新宣告时间（限制频率，避免其他设备看到反复上下线）
    last_reannounce: Option<std::time::Instant>,
}

impl DiscoveryService {
    pub fn new(
        device_id: String,
        device_name: String,
        hostname: String,
        platform: String,
        tcp_port: u16,
    ) -> AppResult<Self> {
        Self::new_with_ip(device_id, device_name, hostname, platform, tcp_port, None)
    }

    /// 创建服务并指定本机 IP（Android 上通过 JNI 获取 WiFi IP）
    pub fn new_with_ip(
        device_id: String,
        device_name: String,
        hostname: String,
        platform: String,
        tcp_port: u16,
        my_ip: Option<std::net::IpAddr>,
    ) -> AppResult<Self> {
        let mdns = ServiceDaemon::new()
            .map_err(|e| crate::error::AppError::Discovery(format!("创建 mDNS 服务失败: {}", e)))?;

        let (event_tx, _) = broadcast::channel(64);

        Ok(Self {
            mdns,
            service_name: device_id.clone(),
            discovered_devices: Arc::new(DashMap::new()),
            event_tx,
            device_id,
            device_name,
            hostname,
            platform,
            tcp_port,
            my_ip,
            browser_started: false,
            last_reannounce: None,
        })
    }

    /// 启动 mDNS 服务注册 + 浏览器
    pub fn start(&mut self) -> AppResult<()> {
        // 注册本机服务
        let service_info = self.build_service_info()?;
        self.mdns
            .register(service_info)
            .map_err(|e| crate::error::AppError::Discovery(format!("注册 mDNS 服务失败: {}", e)))?;

        tracing::info!(
            "mDNS 服务已注册: {} (设备: {}, 端口: {})",
            self.service_name,
            self.device_name,
            self.tcp_port
        );

        // 启动浏览器
        // 仅在首次启动时创建浏览器线程，改名时仅重新注册服务
        if !self.browser_started {
            self.browser_started = true;

            let rx = self
                .mdns
                .browse(SERVICE_TYPE)
                .map_err(|e| crate::error::AppError::Discovery(format!("mDNS 浏览失败: {}", e)))?;

            let devices = self.discovered_devices.clone();
            let event_tx = self.event_tx.clone();
            let my_device_id = self.device_id.clone();

            // 克隆 ServiceDaemon 以便浏览器线程在出错时重新 browse
            let mdns_daemon = self.mdns.clone();

            // 在单独线程中运行同步的 mDNS 事件循环
            std::thread::spawn(move || {
                // 使用 loop 包装，出错时可以重试 browse
                Self::browse_loop(
                    mdns_daemon,
                    rx,
                    devices,
                    event_tx,
                    my_device_id,
                );
            });

            Ok(())
        } else {
            // 浏览器已启动，仅重新注册服务（改名时调用）
            Ok(())
        }
    }

    /// 更新设备名（用于 mDNS 重新注册时）
    pub fn set_device_name(&mut self, name: String) {
        self.device_name = name;
    }

    /// 停止服务
    pub fn stop(&self) -> AppResult<()> {
        // 构造完整服务名: "{instance_name}.{service_type}"
        let fullname = format!("{}.{}", self.service_name, SERVICE_TYPE);
        let receiver = self.mdns.unregister(&fullname);
        if let Err(e) = receiver {
            tracing::warn!("取消注册 mDNS 服务失败: {}", e);
        }
        Ok(())
    }

    /// 构造本机 mDNS 服务信息
    fn build_service_info(&self) -> Result<ServiceInfo, crate::error::AppError> {
        let txt_properties = [
            ("device_id", self.device_id.as_str()),
            ("device_name", self.device_name.as_str()),
            ("platform", self.platform.as_str()),
            ("tcp_port", &self.tcp_port.to_string()),
        ];

        let my_ip = self.my_ip.unwrap_or_else(|| {
            if_addrs::get_if_addrs()
                .ok()
                .and_then(|ifaces| {
                    ifaces
                        .into_iter()
                        .find(|i| !i.is_loopback() && matches!(i.addr, if_addrs::IfAddr::V4(_)))
                        .map(|i| i.ip())
                })
                .unwrap_or_else(|| std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
        });

        ServiceInfo::new(
            SERVICE_TYPE,
            &self.service_name,
            &self.hostname,
            my_ip,
            self.tcp_port,
            &txt_properties[..],
        )
        .map_err(|e| {
            crate::error::AppError::Discovery(format!("创建 ServiceInfo 失败: {}", e))
        })
    }

    /// 重新宣告 mDNS 服务（先注销再注册，强制刷新 SRV/TXT 公告）
    /// 限频：至少间隔 60 秒，避免其他设备看到反复上下线
    pub fn reannounce(&mut self) -> AppResult<()> {
        let now = std::time::Instant::now();
        if let Some(last) = self.last_reannounce {
            if now - last < std::time::Duration::from_secs(60) {
                return Ok(()); // 未到间隔，跳过
            }
        }
        self.last_reannounce = Some(now);

        // 先注销旧服务（强制其他设备清除缓存）
        let fullname = format!("{}.{}", self.service_name, SERVICE_TYPE);
        if let Err(e) = self.mdns.unregister(&fullname) {
            tracing::debug!("reannounce 注销旧服务失败: {}", e);
        }
        // 立即重新注册（触发完整的 mDNS 公告含 SRV/TXT）
        let service_info = self.build_service_info()?;
        self.mdns
            .register(service_info)
            .map_err(|e| crate::error::AppError::Discovery(format!("重新注册 mDNS 失败: {}", e)))?;
        tracing::debug!("mDNS 服务已重新宣告");
        Ok(())
    }

    /// 重新发送 mDNS 查询 + 重新宣告本机服务
    /// 启动独立线程处理返回事件，10s 后自动退出
    pub fn rebrowse(&mut self) -> AppResult<()> {
        // 重新宣告本机服务（强制刷新 SRV/TXT，解决 Windows mdns-sd 响应延迟）
        if let Err(e) = self.reannounce() {
            tracing::debug!("rebrowse 中重新宣告失败: {}", e);
        }

        let rx = self
            .mdns
            .browse(SERVICE_TYPE)
            .map_err(|e| {
                crate::error::AppError::Discovery(format!("mDNS rebrowse 失败: {}", e))
            })?;

        let devices = self.discovered_devices.clone();
        let event_tx = self.event_tx.clone();
        let my_device_id = self.device_id.clone();

        // 启动独立线程处理 rebrowse 返回的事件
        // rx 在线程退出时自动 drop，浏览器将被清理
        std::thread::spawn(move || {
            tracing::debug!("mDNS rebrowse 线程启动");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    tracing::debug!("mDNS rebrowse 线程超时退出");
                    break;
                }
                // 使用 recv_timeout 替代 recv()：最多阻塞 1 秒
                // recv_timeout 在超时时返回 Err（重新检查 deadline）
                // 在浏览器断开时也返回 Err（下次循环 deadline 到期后退出）
                match rx.recv_timeout(remaining.min(std::time::Duration::from_secs(1))) {
                    Ok(event) => {
                        Self::process_service_event(
                            event,
                            &devices,
                            &event_tx,
                            &my_device_id,
                        );
                    }
                    Err(_) => {
                        // 超时或断开 → 回到循环开头检查 deadline
                        // 断开时浏览器 stopped，recv_timeout 立即返回 Err，
                        // 但 deadline 未到期时会快速自旋直到到期（最多 10 秒内无害）
                        continue;
                    }
                }
            }
            // rx 在此 drop → 浏览器被清理
        });

        tracing::debug!("mDNS rebrowse 已发送");
        Ok(())
    }

    /// 订阅发现事件
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }

    /// 获取当前在线设备列表
    pub fn list_devices(&self) -> Vec<DiscoveredDevice> {
        self.discovered_devices
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }

    /// 获取在线设备数量
    pub fn device_count(&self) -> usize {
        self.discovered_devices.len()
    }

    /// 获取发现设备列表的引用（供外部使用）
    pub fn discovered_devices_map(&self) -> &Arc<DashMap<String, DiscoveredDevice>> {
        &self.discovered_devices
    }

    /// 清理超过指定秒数未更新的过期设备
    pub fn clean_stale_devices(&self, max_age_secs: u64) -> Vec<String> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let mut removed = Vec::new();
        self.discovered_devices.retain(|id, device| {
            if device.last_seen < cutoff {
                tracing::info!("清理过期设备: {} ({}), 最后出现: {}", device.device_name, id, device.last_seen);
                removed.push(id.clone());
                false
            } else {
                true
            }
        });
        removed
    }
}

impl DiscoveryService {
    /// 处理单个 mDNS 服务事件（主浏览器和 rebrowse 线程共用）
    fn process_service_event(
        event: ServiceEvent,
        devices: &DashMap<String, DiscoveredDevice>,
        event_tx: &broadcast::Sender<DiscoveryEvent>,
        my_device_id: &str,
    ) {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                let device_id = info
                    .get_property("device_id")
                    .map(|v| {
                        let s = v.to_string();
                        s.strip_prefix("device_id=").unwrap_or(&s).to_string()
                    })
                    .unwrap_or_default();

                // 跳过本机（含 TXT 未就绪导致的空 ID）
                if device_id == my_device_id || device_id.is_empty() {
                    return;
                }

                let device_name = info
                    .get_property("device_name")
                    .map(|v| {
                        let s = v.to_string();
                        s.strip_prefix("device_name=").unwrap_or(&s).to_string()
                    })
                    .unwrap_or_else(|| "未知设备".to_string());

                let platform = info
                    .get_property("platform")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let ip_address = info
                    .get_addresses()
                    .iter()
                    .next()
                    .map(|a| a.to_string())
                    .unwrap_or_default();

                // 更新 last_seen + 保留 first_seen（用于回退超时判断）
                let first_seen = if let Some(existing) = devices.get(&device_id) {
                    let fs = existing.first_seen;
                    drop(existing);
                    if let Some(mut e) = devices.get_mut(&device_id) {
                        e.last_seen = chrono::Utc::now();
                    }
                    fs // 保留首次发现时间
                } else {
                    chrono::Utc::now() // 新设备
                };

                let device = DiscoveredDevice {
                    device_id: device_id.clone(),
                    device_name,
                    hostname: info.get_hostname().to_string(),
                    platform,
                    ip_address,
                    tcp_port: info.get_port(),
                    last_seen: chrono::Utc::now(),
                    first_seen,
                };

                tracing::info!(
                    "发现设备: {} ({}) @ {}:{}",
                    device.device_name,
                    device.platform,
                    device.ip_address,
                    device.tcp_port
                );

                let _ = event_tx.send(DiscoveryEvent::DeviceFound(device.clone()));
                devices.insert(device_id, device);
            }
            ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                let device_id = fullname
                    .strip_suffix(&format!(".{}", SERVICE_TYPE))
                    .unwrap_or(&fullname);

                if device_id != my_device_id {
                    tracing::info!("设备离线: {}", device_id);
                    let _ = event_tx.send(DiscoveryEvent::DeviceLost(device_id.to_string()));
                    devices.remove(device_id);
                }
            }
            _ => {}
        }
    }

    /// mDNS 浏览器事件循环（独立函数，运行在单独线程中）
    /// 出错时自动重试 browse，使用指数退避策略
    fn browse_loop(
        daemon: ServiceDaemon,
        mut rx: mdns_sd::Receiver<ServiceEvent>,
        devices: Arc<DashMap<String, DiscoveredDevice>>,
        event_tx: broadcast::Sender<DiscoveryEvent>,
        my_device_id: String,
    ) {
        let mut backoff_secs: u64 = 5; // 初始退避 5 秒
        const MAX_BACKOFF_SECS: u64 = 60;

        loop {
            match rx.recv() {
                Ok(event) => {
                    // 成功收到事件时重置退避时间
                    backoff_secs = 5;
                    Self::process_service_event(event, &devices, &event_tx, &my_device_id);
                }
                Err(e) => {
                    tracing::error!(
                        "mDNS 浏览错误: {}, {} 秒后重试",
                        e,
                        backoff_secs
                    );
                    std::thread::sleep(Duration::from_secs(backoff_secs));

                    // 尝试停止旧浏览器（忽略错误，可能已经停止）
                    if let Err(e) = daemon.stop_browse(SERVICE_TYPE) {
                        tracing::debug!("stop_browse 失败（可能已停止）: {}", e);
                    }

                    // 重新浏览
                    match daemon.browse(SERVICE_TYPE) {
                        Ok(new_rx) => {
                            tracing::info!("mDNS 浏览器已重新启动");
                            rx = new_rx;
                            // 指数退避，上限 MAX_BACKOFF_SECS
                            backoff_secs =
                                std::cmp::min(backoff_secs * 2, MAX_BACKOFF_SECS);
                        }
                        Err(e) => {
                            tracing::error!(
                                "mDNS 重新浏览失败: {}, 浏览器线程退出",
                                e
                            );
                            break;
                        }
                    }
                }
            }
        }
    }
}
