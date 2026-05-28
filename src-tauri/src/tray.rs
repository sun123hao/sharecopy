use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager, Runtime,
};

/// 创建系统托盘
pub fn create_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    // 构建菜单项
    let show_window = MenuItemBuilder::with_id("show_window", "显示主窗口").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出 ShareCopy").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_window)
        .separator()
        .item(&quit)
        .build()?;

    // 创建托盘图标
    let icon = create_tray_icon();

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            tracing::info!("托盘菜单点击: {}", id);
            match id {
                "show_window" => show_main_window(app),
                "quit" => {
                    tracing::info!("退出 ShareCopy");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    // 保持托盘图标存活
    // 注意：不能 forget()，否则 macOS 上菜单事件可能无法正常分发
    // 将 tray 绑定到 app 上以保持其生命周期
    app.try_state::<TrayHolder<R>>();
    app.manage(TrayHolder(std::sync::Mutex::new(Some(_tray))));

    Ok(())
}

/// 托盘图标持有者 —— 防止被 drop
struct TrayHolder<R: Runtime>(std::sync::Mutex<Option<tauri::tray::TrayIcon<R>>>);

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
