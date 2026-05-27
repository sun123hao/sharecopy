use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};

/// 创建系统托盘，返回 TrayIcon 句柄（调用方负责保持其存活）
pub fn create_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    // 构建菜单项
    let show_window = MenuItemBuilder::with_id("show_window", "显示主窗口").build(app)?;
    let toggle_sync = MenuItemBuilder::with_id("toggle_sync", "暂停同步").build(app)?;
    let open_settings = MenuItemBuilder::with_id("open_settings", "打开设置").build(app)?;
    let send_file = MenuItemBuilder::with_id("send_file", "发送文件...").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出 ShareCopy").build(app)?;

    // 设备子菜单（动态更新）
    let devices_menu = SubmenuBuilder::new(app, "在线设备")
        .text("devices_placeholder", "搜索中...")
        .separator()
        .text("device_count", "共 0 台设备在线")
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&show_window)
        .separator()
        .item(&toggle_sync)
        .separator()
        .item(&devices_menu)
        .separator()
        .item(&send_file)
        .item(&open_settings)
        .separator()
        .item(&quit)
        .build()?;

    // 创建托盘图标
    let icon = create_tray_icon();

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "show_window" | "open_settings" => show_main_window(app),
                "toggle_sync" => {
                    tracing::info!("切换同步状态");
                }
                "send_file" => {
                    tracing::info!("发送文件");
                }
                "quit" => {
                    tracing::info!("退出 ShareCopy");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            match event {
                TrayIconEvent::Click {
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    show_main_window(tray.app_handle());
                }
                _ => {}
            }
        })
        .build(app)?;

    // 关键：阻止 TrayIcon 被 drop，否则托盘图标会消失
    std::mem::forget(tray);

    Ok(())
}

/// 从嵌入的原始 RGBA 数据加载托盘图标（32x32）
fn create_tray_icon() -> Image<'static> {
    const SIZE: u32 = 32;
    let rgba = include_bytes!("../icons/tray-icon.rgba");
    Image::new(rgba, SIZE, SIZE)
}

fn show_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    match app.get_webview_window("main") {
        Some(window) => {
            tracing::info!("显示主窗口");
            let _ = window.show();
            let _ = window.set_focus();
        }
        None => {
            tracing::error!("找不到主窗口 (label: main)");
        }
    }
}

