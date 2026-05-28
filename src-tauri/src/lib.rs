use std::sync::Arc;
use tauri::{Emitter, Manager};
#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tokio::sync::mpsc;

mod error;
mod protocol;
mod clipboard;
#[cfg(target_os = "macos")]
mod clipboard_macos;
#[cfg(target_os = "windows")]
mod clipboard_windows;
mod discovery;
mod network;
mod sync;
mod transfer;
mod config;
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
async fn get_devices(_state: tauri::State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    // 暂时返回空列表，后续通过 event 推送
    Ok(vec![])
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
    let mut config = state.config.write().await;
    config.device_name = name;
    config.save().map_err(|e| e.to_string())
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
    *config = new_config;
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
        let p = std::path::PathBuf::from(&path);
        state
            .transfer_manager
            .send_file(&p, &target)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // 初始化日志
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();

            tracing::info!("ShareCopy 启动中...");

            // 加载配置
            let app_config = AppConfig::load().unwrap_or_default();
            let device_id = app_config.device_id.clone();
            let device_name = app_config.device_name.clone();
            let tcp_port = app_config.tcp_port;
            let save_dir = app_config.save_dir.clone();

            // 获取本机信息
            let hostname = {
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
            let platform = std::env::consts::OS.to_string();

            tracing::info!("设备: {} ({}), 端口: {}", device_name, platform, tcp_port);

            // 创建网络事件通道
            let (network_event_tx, network_event_rx) =
                mpsc::unbounded_channel::<NetworkEvent>();

            // 创建网络管理器
            let network = Arc::new(NetworkManager::new(
                device_id.clone(),
                device_name.clone(),
                tcp_port,
                network_event_tx,
            ));

            // 创建剪贴板后端
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

            // 创建剪贴板监视器（run() 使用 &self，可安全放入 Arc）
            let (clipboard_tx, clipboard_rx) = mpsc::unbounded_channel();
            let watcher = Arc::new(ClipboardWatcher::new(
                clipboard_backend,
                clipboard_tx,
                app_config.poll_interval_active_ms,
                app_config.poll_interval_idle_ms,
            ));

            // 创建设备发现服务
            let mut discovery_service = match DiscoveryService::new(
                device_id.clone(),
                device_name.clone(),
                hostname,
                platform,
                tcp_port,
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

            // 创建文件传输管理器
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

            // 拦截窗口关闭事件 —— 关闭按钮改为隐藏到托盘
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

            // 构建系统托盘
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
        ])
        .build(tauri::generate_context!())
        .expect("ShareCopy 启动失败")
        .run(|app_handle, event| {
            // macOS: 点击 Dock 图标时恢复主窗口
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            let _ = event;
        });
}
