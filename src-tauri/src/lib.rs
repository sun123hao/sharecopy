use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri::RunEvent;
use tokio::sync::mpsc;

pub mod error;
pub mod protocol;
pub mod clipboard;
#[cfg(target_os = "macos")]
mod clipboard_macos;
#[cfg(target_os = "windows")]
mod clipboard_windows;
#[cfg(target_os = "android")]
mod clipboard_android;
#[cfg(target_os = "android")]
mod android_network;
#[cfg(target_os = "android")]
mod android_file;
pub mod discovery;
pub mod network;
pub mod sync;
pub mod transfer;
pub mod config;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tray;

use clipboard::ClipboardWatcher;
use config::AppConfig;
use discovery::DiscoveryService;
use network::{NetworkEvent, NetworkManager};
use sync::SyncEngine;
use transfer::FileTransferManager;

pub struct AppState {
    pub sync_engine: Arc<SyncEngine>,
    pub transfer_manager: Arc<FileTransferManager>,
    pub network: Arc<NetworkManager>,
    pub config: Arc<tokio::sync::RwLock<AppConfig>>,
}

// ── Tauri 命令 ─────────────────────────

#[tauri::command]
async fn get_devices(state: tauri::State<'_, AppState>) -> Result<Vec<discovery::DiscoveredDevice>, String> {
    // 已连接设备始终显示；未连接设备 90s 内 mDNS 发现才显示（避免退出残留）
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(90);
    let mut devices = state.sync_engine.get_discovered_devices()
        .into_iter()
        .filter(|d| d.last_seen >= cutoff || state.network.is_connected(&d.device_id))
        .collect::<Vec<_>>();

    // 补充：TCP 已连接但 mDNS 不可达的设备
    let existing_ids: std::collections::HashSet<String> = devices.iter().map(|d| d.device_id.clone()).collect();
    for id in state.network.connected_device_ids() {
        if !existing_ids.contains(&id) {
            if let Some(info) = state.sync_engine.get_connected_device_info(&id) {
                devices.push(info);
            }
        }
    }
    Ok(devices)
}

#[tauri::command]
async fn toggle_sync(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.sync_engine.toggle_sync())
}

#[tauri::command]
async fn update_device_name(
    state: tauri::State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    tracing::info!("update_device_name 被调用: {}", name);
    let mut config = state.config.write().await;
    config.device_name = name.clone();
    config.name_customized = true; // 标记为用户自定义，防止 Android 覆盖
    config.save().map_err(|e| {
        tracing::error!("保存配置失败: {}", e);
        e.to_string()
    })?;
    tracing::info!("配置已保存, 通知 mDNS 更新...");
    // 通知同步引擎更新设备名（重新注册 mDNS）
    state.sync_engine.update_device_name(&name);
    tracing::info!("设备名更新完成: {}", name);
    Ok(())
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn update_config(
    state: tauri::State<'_, AppState>,
    new_config: AppConfig,
) -> Result<(), String> {
    let mut config = state.config.write().await;
    // 检测 save_dir 是否被用户修改，标记为自定义以防 Android 启动时覆盖
    if new_config.save_dir != config.save_dir {
        config.save_dir = new_config.save_dir;
        config.save_dir_customized = true;
    }
    // 同步其他字段
    config.device_name = new_config.device_name;
    config.name_customized = new_config.name_customized;
    config.device_id = new_config.device_id;
    config.tcp_port = new_config.tcp_port;
    config.auto_start = new_config.auto_start;
    config.sync_enabled = new_config.sync_enabled;
    config.auto_accept_files = new_config.auto_accept_files;
    config.poll_interval_active_ms = new_config.poll_interval_active_ms;
    config.poll_interval_idle_ms = new_config.poll_interval_idle_ms;
    config.save().map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_sync_stats(
    state: tauri::State<'_, AppState>,
) -> Result<sync::SyncStats, String> {
    Ok(state.sync_engine.get_stats())
}

#[tauri::command]
async fn send_files(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    target: String,
) -> Result<(), String> {
    for path in paths {
        // Android: 将 content:// URI 转换为可读取的临时文件路径
        #[cfg(target_os = "android")]
        let resolved = android_file::resolve_content_uri(&path).map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "android"))]
        let resolved = path;

        let p = std::path::PathBuf::from(&resolved);
        state
            .transfer_manager
            .send_file(&p, &target)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 主动刷新 mDNS 发现（重新 browse，发送新查询）
#[tauri::command]
async fn refresh_discovery(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.sync_engine.refresh_discovery();
    Ok(())
}

/// 取消正在进行的文件传输
#[tauri::command]
async fn cancel_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    state
        .transfer_manager
        .cancel_transfer(&transfer_id)
        .map_err(|e| e.to_string())
}

/// 获取可用的保存目录路径（Android 返回真实目录，桌面端返回空）
#[tauri::command]
async fn get_android_save_dirs() -> Result<Vec<String>, String> {
    #[cfg(target_os = "android")]
    {
        android_file::get_save_directories().map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn get_clipboard_history(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<sync::ClipboardHistoryEntry>, String> {
    Ok(state.sync_engine.get_history())
}

#[tauri::command]
async fn copy_from_history(
    state: tauri::State<'_, AppState>,
    entry_id: String,
) -> Result<(), String> {
    let history = state.sync_engine.get_history();
    if let Some(entry) = history.iter().find(|e| e.id == entry_id) {
        let content = match entry.entry_type.as_str() {
            "text" => clipboard::ClipboardContent::Text(entry.content.clone()),
            "image" => {
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD
                    .decode(&entry.content)
                    .map_err(|e| format!("Base64 解码失败: {}", e))?;
                // arboard 写入 PNG 数据时无需精确宽高
                clipboard::ClipboardContent::Image { width: 0, height: 0, data }
            }
            _ => return Err(format!("不支持的类型: {}", entry.entry_type)),
        };
        state.sync_engine.write_to_clipboard(&content)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn is_sync_enabled(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.sync_engine.is_sync_enabled())
}

// ── 应用入口 ──────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_log::Builder::default().build());

    // 桌面端专属插件
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--minimized"]),
    ));

    builder
        .setup(|app| {
            // 初始化日志（try_init 避免 Tauri 已注册 subscriber 时 panic）
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .try_init();

            tracing::info!("ShareCopy 启动中...");

            // 加载配置
            let mut app_config = AppConfig::load().unwrap_or_default();
            let device_id = app_config.device_id.clone();
            let tcp_port = app_config.tcp_port;
            let platform = std::env::consts::OS.to_string();

            // 创建网络事件通道
            let (network_event_tx, network_event_rx) =
                mpsc::unbounded_channel::<NetworkEvent>();

            // ── 先创建剪贴板后端（Android 上这会使 JVM 就绪） ──
            #[cfg(target_os = "macos")]
            let clipboard_backend: Box<dyn clipboard::ClipboardBackend> = {
                match clipboard_macos::MacOSClipboardBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(e) => {
                        tracing::error!("无法创建 macOS 剪贴板后端: {}", e);
                        return Err(Box::new(e));
                    }
                }
            };
            #[cfg(target_os = "windows")]
            let clipboard_backend: Box<dyn clipboard::ClipboardBackend> = {
                match clipboard_windows::WindowsClipboardBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(e) => {
                        tracing::error!("无法创建 Windows 剪贴板后端: {}", e);
                        return Err(Box::new(e));
                    }
                }
            };
            #[cfg(target_os = "android")]
            let clipboard_backend: Box<dyn clipboard::ClipboardBackend> = {
                match clipboard_android::AndroidClipboardBackend::new() {
                    Ok(b) => Box::new(b),
                    Err(e) => {
                        tracing::error!("无法创建 Android 剪贴板后端: {}", e);
                        return Err(Box::new(e));
                    }
                }
            };

            // ── 获取本机信息（Android 上在 JVM 就绪后通过 JNI 获取） ──
            #[cfg(target_os = "android")]
            {
                // 仅在用户从未自定义名称时，用 Build.MODEL 填充
                if !app_config.name_customized
                    && (app_config.device_name.is_empty() || app_config.device_name == "localhost")
                {
                    match android_network::get_device_model() {
                        Ok(name) if !name.is_empty() => {
                            tracing::info!("Android 设备型号: {}", name);
                            app_config.device_name = name;
                            let _ = app_config.save();
                        }
                        Ok(_) => tracing::warn!("Android 设备型号为空"),
                        Err(e) => tracing::warn!("无法获取 Android 设备型号: {}", e),
                    }
                }
            }
            let device_name = app_config.device_name.clone();

            let hostname = {
                #[cfg(target_os = "android")]
                let h = {
                    // Android 上使用设备 ID 构建唯一 hostname
                    format!("android-{}.local.", &device_id[..std::cmp::min(8, device_id.len())])
                };
                #[cfg(not(target_os = "android"))]
                let h = hostname::get()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                // mdns-sd 要求 hostname 以 ".local." 结尾
                if h.ends_with(".local.") {
                    h
                } else if h.ends_with(".local") {
                    format!("{}.", h)
                } else {
                    format!("{}.local.", h.trim_end_matches('.'))
                }
            };

            // Android: 获取 WakeLock + WifiLock 防止息屏后 CPU/WiFi 休眠
            #[cfg(target_os = "android")]
            {
                if let Err(e) = android_network::acquire_wake_lock() {
                    tracing::warn!("获取 WakeLock 失败: {}", e);
                }
                if let Err(e) = android_network::acquire_wifi_lock() {
                    tracing::warn!("获取 WiFi Lock 失败: {}", e);
                }
            }

            // Android: 用 JNI 获取外部文件目录作为默认保存路径
            // 仅在用户未自定义过保存目录时才自动覆盖（避免覆盖用户设置）
            #[cfg(target_os = "android")]
            {
                if !app_config.save_dir_customized {
                    let default_save = android_file::get_default_save_dir();
                    match default_save {
                        Ok(dir) => {
                            if std::fs::create_dir_all(&dir).is_ok() {
                                tracing::info!("Android 默认保存路径: {}", dir.display());
                                app_config.save_dir = dir;
                            }
                        }
                        Err(e) => tracing::warn!("获取 Android 保存目录失败: {}", e),
                    }
                } else {
                    tracing::info!("Android 保存目录已由用户自定义: {}", app_config.save_dir.display());
                }
            }

            tracing::info!("设备: {} ({}), 端口: {}", device_name, platform, tcp_port);

            // 创建网络管理器
            let network = Arc::new(NetworkManager::new(
                device_id.clone(),
                device_name.clone(),
                tcp_port,
                network_event_tx,
            ));

            // 创建剪贴板监视器（run() 使用 &self，可安全放入 Arc）
            let (clipboard_tx, clipboard_rx) = mpsc::unbounded_channel();
            let watcher = Arc::new(ClipboardWatcher::new(
                clipboard_backend,
                clipboard_tx,
                app_config.poll_interval_active_ms,
                app_config.poll_interval_idle_ms,
            ));

            // 创建设备发现服务
            // Android: 通过 JNI 获取 WiFi IP，if_addrs crate 在 Android 上不可靠
            #[cfg(target_os = "android")]
            let my_ip = android_network::get_wifi_ip()
                .ok()
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
            #[cfg(not(target_os = "android"))]
            let my_ip: Option<std::net::IpAddr> = None;

            let mut discovery_service = match DiscoveryService::new_with_ip(
                device_id.clone(),
                device_name.clone(),
                hostname,
                platform,
                tcp_port,
                my_ip,
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("无法创建设备发现服务: {}", e);
                    return Err(Box::new(e));
                }
            };

            let discovery_rx = discovery_service.subscribe();

            // 启动设备发现（非致命错误，失败时仅记录日志）
            if let Err(e) = discovery_service.start() {
                tracing::warn!("设备发现启动失败（后台功能不可用）: {}", e);
            }

            // 创建文件传输管理器（save_dir 在 Android 覆盖后取值）
            let save_dir = app_config.save_dir.clone();
            let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();
            let transfer_manager = Arc::new(FileTransferManager::new(
                save_dir,
                network.clone(),
                progress_tx,
            ));

            // 创建同步引擎
            let sync_engine = Arc::new(SyncEngine::new(
                device_id.clone(),
                watcher.clone(),
                discovery_service,
                network.clone(),
                transfer_manager.clone(),
                app.app_handle().clone(),
            ));

            // 注入应用状态
            let app_state = AppState {
                sync_engine: sync_engine.clone(),
                transfer_manager: transfer_manager.clone(),
                network: network.clone(),
                config: Arc::new(tokio::sync::RwLock::new(app_config)),
            };
            app.manage(app_state);

            // 桌面端：拦截窗口关闭改为隐藏到托盘
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Some(window) = app.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                });
            }

            // 启动网络服务器
            let network_clone = network.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = network_clone.start().await {
                    tracing::error!("网络服务器启动失败: {}", e);
                }
            });

            // 启动同步引擎
            let engine = sync_engine.clone();
            tauri::async_runtime::spawn(async move {
                engine.run(clipboard_rx, network_event_rx, discovery_rx).await;
            });

            // 启动剪贴板监视器
            let watcher_clone = watcher.clone();
            tauri::async_runtime::spawn(async move {
                watcher_clone.run().await;
            });

            // 转发传输进度事件到前端
            let app_handle = app.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    let _ = app_handle.emit("transfer-progress", &progress);
                }
            });

            // 桌面端：构建系统托盘
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tray::create_tray(app)?;

            tracing::info!("ShareCopy 启动完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_devices,
            toggle_sync,
            update_device_name,
            get_config,
            update_config,
            get_sync_stats,
            send_files,
            get_clipboard_history,
            copy_from_history,
            is_sync_enabled,
            get_android_save_dirs,
            refresh_discovery,
            cancel_transfer,
        ])
        .build(tauri::generate_context!())
        .expect("ShareCopy 启动失败")
        .run(|app_handle, event| {
            #[allow(unused_variables)]
            // macOS: 点击 Dock 图标恢复主窗口
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            // Android/移动端：进入后台时暂停剪贴板轮询，回到前台时恢复
            #[cfg(any(target_os = "android", target_os = "ios"))]
            match &event {
                RunEvent::Exit => {
                    tracing::info!("移动端应用退出");
                }
                _ => {}
            }
            let _ = event;
        });
}
