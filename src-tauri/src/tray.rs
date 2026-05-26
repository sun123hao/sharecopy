use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, Runtime,
};

/// 创建系统托盘
pub fn create_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    // 构建菜单项
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
    // 使用默认图标（32x32 的简单 PNG）
    let icon = create_tray_icon();

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                "toggle_sync" => {
                    tracing::info!("切换同步状态");
                    // TODO: 调用同步引擎 toggle
                }
                "open_settings" => {
                    // 显示设置窗口
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "send_file" => {
                    tracing::info!("发送文件");
                    // TODO: 打开文件选择器
                }
                "quit" => {
                    tracing::info!("退出 ShareCopy");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// 创建一个简单的托盘图标（16x16 像素，蓝色圆角方块）
fn create_tray_icon() -> Image<'static> {
    // 32x32 RGBA 简单图标
    let size = 32u32;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            // 圆角矩形
            let margin = 4;
            let in_rect = x >= margin && x < size - margin && y >= margin && y < size - margin;

            if in_rect {
                pixels.push(59);  // R
                pixels.push(130); // G
                pixels.push(246); // B
                pixels.push(255); // A
            } else {
                pixels.push(0);
                pixels.push(0);
                pixels.push(0);
                pixels.push(0);
            }
        }
    }

    Image::new_owned(
        pixels,
        size,
        size,
    )
}
