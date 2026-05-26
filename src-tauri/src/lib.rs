use tauri::Manager;

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

pub struct AppState {
    pub config: std::sync::Arc<tokio::sync::RwLock<config::AppConfig>>,
}

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
            let app_config = config::AppConfig::load().unwrap_or_default();
            let app_state = AppState {
                config: std::sync::Arc::new(tokio::sync::RwLock::new(app_config)),
            };
            app.manage(app_state);

            // 构建系统托盘
            tray::create_tray(app)?;

            tracing::info!("ShareCopy 启动完成");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ShareCopy 启动失败");
}
