use arboard::Clipboard;
use arboard::ImageData;
use objc2::msg_send;
use objc2::class;

use crate::clipboard::{ClipboardBackend, ClipboardContent};
use crate::error::{AppError, AppResult};

pub struct MacOSClipboardBackend;
// arboard 内部使用同步原语，可以在不可变引用上操作

impl MacOSClipboardBackend {
    pub fn new() -> AppResult<Self> {
        // 验证剪贴板可访问
        let _ = Clipboard::new()
            .map_err(|e| AppError::Clipboard(format!("无法打开剪贴板: {}", e)))?;
        Ok(Self)
    }
}

impl ClipboardBackend for MacOSClipboardBackend {
    fn read(&self) -> AppResult<ClipboardContent> {
        let mut clipboard = Clipboard::new()
            .map_err(|e| AppError::Clipboard(format!("无法打开剪贴板: {}", e)))?;

        // 先尝试读取图片
        if let Ok(image) = clipboard.get_image() {
            let width = image.width as u32;
            let height = image.height as u32;
            let rgba = image.bytes.to_vec();

            let img = image::RgbaImage::from_raw(width, height, rgba)
                .ok_or_else(|| AppError::Clipboard("无法解析剪贴板图片数据".into()))?;

            let mut png_bytes: Vec<u8> = Vec::new();
            let dynamic_img = image::DynamicImage::ImageRgba8(img);
            dynamic_img
                .write_to(
                    &mut std::io::Cursor::new(&mut png_bytes),
                    image::ImageFormat::Png,
                )
                .map_err(|e| AppError::Image(e))?;

            return Ok(ClipboardContent::Image {
                width,
                height,
                data: png_bytes,
            });
        }

        // 再尝试读取文本
        if let Ok(text) = clipboard.get_text() {
            if !text.is_empty() {
                return Ok(ClipboardContent::Text(text));
            }
        }

        Ok(ClipboardContent::None)
    }

    fn write(&self, content: &ClipboardContent) -> AppResult<()> {
        let mut clipboard = Clipboard::new()
            .map_err(|e| AppError::Clipboard(format!("无法打开剪贴板: {}", e)))?;

        match content {
            ClipboardContent::Text(text) => {
                clipboard
                    .set_text(text)
                    .map_err(|e| AppError::Clipboard(format!("写入文本失败: {}", e)))?;
            }
            ClipboardContent::Image {
                width,
                height,
                data,
            } => {
                let img = image::load_from_memory(data)
                    .map_err(|e| AppError::Image(e))?
                    .into_rgba8();

                let img_data = ImageData {
                    width: *width as usize,
                    height: *height as usize,
                    bytes: std::borrow::Cow::Owned(img.into_raw()),
                };

                clipboard
                    .set_image(img_data)
                    .map_err(|e| AppError::Clipboard(format!("写入图片失败: {}", e)))?;
            }
            ClipboardContent::None => {}
        }

        Ok(())
    }

    /// 直接读取系统 NSPasteboard.changeCount，精确检测每次剪贴板变更
    fn change_count(&self) -> AppResult<u64> {
        unsafe {
            let pasteboard: *const objc2::runtime::AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
            let count: objc2::ffi::NSInteger = msg_send![pasteboard, changeCount];
            Ok(count as u64)
        }
    }
}
