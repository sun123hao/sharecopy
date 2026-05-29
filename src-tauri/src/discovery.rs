use dashmap::DashMap;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::sync::Arc;
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

            // 在单独线程中运行同步的 mDNS 事件循环
            std::thread::spawn(move || {
            loop {
                match rx.recv() {
                    Ok(event) => match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let device_id = info
                                .get_property("device_id")
                                .map(|v| v.to_string())
                                .unwrap_or_default();

                            // 跳过本机（含 TXT 未就绪导致的空 ID）
                            if device_id == my_device_id || device_id.is_empty() {
                                continue;
                            }

                            // mDNS 去重：已发现的设备不重复发送事件
                            if devices.contains_key(&device_id) {
                                continue;
                            }

                            let device_name = info
                                .get_property("device_name")
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "未知设备".to_string());

                            let platform = info
                                .get_property("platform")
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string());

                            let device = DiscoveredDevice {
                                device_id: device_id.clone(),
                                device_name,
                                hostname: info.get_hostname().to_string(),
                                platform,
                                ip_address: info
                                    .get_addresses()
                                    .iter()
                                    .next()
                                    .map(|a| a.to_string())
                                    .unwrap_or_default(),
                                tcp_port: info.get_port(),
                                last_seen: chrono::Utc::now(),
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
                    },
                    Err(e) => {
                        tracing::error!("mDNS 浏览错误: {}", e);
                        break;
                    }
                }
            }
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
}
