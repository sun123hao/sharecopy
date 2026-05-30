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
        })
    }

    /// 启动 mDNS 服务注册 + 浏览器
    pub fn start(&mut self) -> AppResult<()> {
        // 注册本机服务
        let txt_properties = [
            ("device_id", self.device_id.as_str()),
            ("device_name", self.device_name.as_str()),
            ("platform", self.platform.as_str()),
            ("tcp_port", &self.tcp_port.to_string()),
        ];

        // 获取本机 IP（优先使用手动指定的 IP，Android 上通过 JNI 获取）
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

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &self.service_name,
            &self.hostname,
            my_ip,
            self.tcp_port,
            &txt_properties[..],
        )
        .map_err(|e| {
            crate::error::AppError::Discovery(format!("创建 ServiceInfo 失败: {}", e))
        })?;

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
                                continue;
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

                            let _ =
                                event_tx.send(DiscoveryEvent::DeviceFound(device.clone()));
                            devices.insert(device_id, device);
                        }
                        ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                            let device_id = fullname
                                .strip_suffix(&format!(".{}", SERVICE_TYPE))
                                .unwrap_or(&fullname);

                            if device_id != my_device_id {
                                tracing::info!("设备离线: {}", device_id);
                                let _ = event_tx
                                    .send(DiscoveryEvent::DeviceLost(device_id.to_string()));
                                devices.remove(device_id);
                            }
                        }
                        _ => {}
                    }
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
