//! Android 剪贴板后端
//!
//! 通过 JNI 调用 Android ClipboardManager API。
//! Android 10+ 限制后台剪贴板访问，后台时需暂停轮询。

use jni::objects::{GlobalRef, JObject, JValue};
use jni::JavaVM;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64};

use crate::clipboard::{ClipboardBackend, ClipboardContent};
use crate::error::AppResult;

pub struct AndroidClipboardBackend {
    jvm: JavaVM,
    _context: GlobalRef,
}

impl AndroidClipboardBackend {
    pub fn new() -> AppResult<Self> {
        // 通过 JNI_GetCreatedJavaVMs 获取已存在的 JVM（Tauri 会在启动时创建）
        let jvm = find_existing_jvm()?;

        let context_global = {
            let mut env = jvm.attach_current_thread().map_err(|e| {
                crate::error::AppError::Clipboard(format!("JNI attach 失败: {}", e))
            })?;

            // 通过 android.app.ActivityThread 获取 Application Context
            // 这条路可能失败，尝试多种方式
            let context = get_app_context(&mut env)?;
            env.new_global_ref(context).map_err(|e| {
                crate::error::AppError::Clipboard(format!("创建 GlobalRef 失败: {}", e))
            })?
        };

        Ok(Self {
            jvm,
            _context: context_global,
        })
    }

    fn with_jni<F, R>(&self, f: F) -> AppResult<R>
    where
        F: for<'a> FnOnce(&mut jni::JNIEnv<'a>, &JObject<'a>, &JObject<'a>) -> AppResult<R>,
    {
        let mut env = self.jvm.attach_current_thread().map_err(|e| {
            crate::error::AppError::Clipboard(format!("JNI 线程附加失败: {}", e))
        })?;

        let ctx = self._context.as_obj();
        let svc = get_clipboard_service(&mut env, ctx)?;

        f(&mut env, ctx, &svc)
    }
}

// JVM 原始指针（JNI_OnLoad 中缓存），存为 usize 绕过 Send/Sync 限制
pub(crate) static GLOBAL_JVM_PTR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// JNI 加载时由 Android 系统自动调用
#[no_mangle]
pub extern "system" fn JNI_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) -> jni::sys::jint {
    GLOBAL_JVM_PTR.store(vm as usize, std::sync::atomic::Ordering::Release);
    jni::sys::JNI_VERSION_1_6
}

/// 获取已缓存的 JVM
fn find_existing_jvm() -> AppResult<JavaVM> {
    let ptr_val = GLOBAL_JVM_PTR.load(std::sync::atomic::Ordering::Acquire);
    if ptr_val == 0 {
        return Err(crate::error::AppError::Clipboard("JVM 尚未初始化".into()));
    }
    let ptr = ptr_val as *mut jni::sys::JavaVM;
    unsafe { JavaVM::from_raw(ptr) }.map_err(|e| {
        crate::error::AppError::Clipboard(format!("无法从 raw pointer 创建 JavaVM: {}", e))
    })
}

/// 获取 Android Application Context
fn get_app_context<'a>(env: &mut jni::JNIEnv<'a>) -> AppResult<JObject<'a>> {
    // 通过 android.app.ActivityThread.currentActivityThread().getApplication()
    let ath_cls = env
        .find_class("android/app/ActivityThread")
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("find_class ActivityThread: {}", e))
        })?;

    let ath = env
        .call_static_method(
            &ath_cls,
            "currentActivityThread",
            "()Landroid/app/ActivityThread;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("currentActivityThread: {}", e))
        })?;

    if ath.is_null() {
        return Err(crate::error::AppError::Clipboard(
            "ActivityThread.currentActivityThread() 返回 null".into(),
        ));
    }

    let app = env
        .call_method(&ath, "getApplication", "()Landroid/app/Application;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("getApplication: {}", e))
        })?;

    if app.is_null() {
        return Err(crate::error::AppError::Clipboard(
            "getApplication() 返回 null".into(),
        ));
    }

    Ok(app)
}

impl ClipboardBackend for AndroidClipboardBackend {
    fn read(&self) -> AppResult<ClipboardContent> {
        self.with_jni(|env, ctx, svc| {
            let clip = call_method_ret(env, svc, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?;
            if clip.is_null() {
                return Ok(ClipboardContent::None);
            }

            let count = env
                .call_method(&clip, "getItemCount", "()I", &[])
                .and_then(|v| v.i())
                .unwrap_or(0);
            if count == 0 {
                return Ok(ClipboardContent::None);
            }

            let item = call_method_ret(
                env,
                &clip,
                "getItemAt",
                "(I)Landroid/content/ClipData$Item;",
                &[JValue::Int(0)],
            )?;
            if item.is_null() {
                return Ok(ClipboardContent::None);
            }

            if let Ok(text_str) = read_item_text(env, &item) {
                if !text_str.is_empty() {
                    return Ok(ClipboardContent::Text(text_str));
                }
            }

            let uri = call_method_ret(env, &item, "getUri", "()Landroid/net/Uri;", &[])?;
            if !uri.is_null() {
                return read_image_from_uri(env, ctx, &uri);
            }

            Ok(ClipboardContent::None)
        })
    }

    fn write(&self, content: &ClipboardContent) -> AppResult<()> {
        self.with_jni(|env, ctx, svc| match content {
            ClipboardContent::Text(text) => {
                let label = env.new_string("ShareCopy").map_err(|e| {
                    crate::error::AppError::Clipboard(format!("new_string: {}", e))
                })?;
                let text_j = env.new_string(text).map_err(|e| {
                    crate::error::AppError::Clipboard(format!("new_string: {}", e))
                })?;

                let cls = env.find_class("android/content/ClipData").map_err(|e| {
                    crate::error::AppError::Clipboard(format!("find_class ClipData: {}", e))
                })?;

                let clip = env
                    .call_static_method(
                        &cls,
                        "newPlainText",
                        "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                        &[JValue::Object(&label), JValue::Object(&text_j)],
                    )
                    .and_then(|v| v.l())
                    .map_err(|e| {
                        crate::error::AppError::Clipboard(format!("newPlainText: {}", e))
                    })?;

                env.call_method(
                    svc,
                    "setPrimaryClip",
                    "(Landroid/content/ClipData;)V",
                    &[JValue::Object(&clip)],
                )
                .map_err(|e| {
                    crate::error::AppError::Clipboard(format!("setPrimaryClip: {}", e))
                })?;

                Ok(())
            }
            ClipboardContent::Image { data, .. } => {
                write_image_to_clipboard(env, ctx, svc, data)
            }
            ClipboardContent::None => Ok(()),
        })
    }

    fn change_count(&self) -> AppResult<u64> {
        self.read().map(|content| {
            let hash = content.content_hash();
            let digest = Sha256::digest(hash.as_bytes());
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&digest[..8]);
            u64::from_be_bytes(buf)
        })
    }
}

// ── JNI 辅助函数 ──────────────────────────────

fn get_clipboard_service<'a>(
    env: &mut jni::JNIEnv<'a>,
    context: &JObject<'a>,
) -> AppResult<JObject<'a>> {
    let cls = env.find_class("android/content/Context").map_err(|e| {
        crate::error::AppError::Clipboard(format!("find_class Context: {}", e))
    })?;

    let svc_name = env
        .get_static_field(&cls, "CLIPBOARD_SERVICE", "Ljava/lang/String;")
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("CLIPBOARD_SERVICE: {}", e))
        })?;

    env.call_method(
        context,
        "getSystemService",
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JValue::Object(&svc_name)],
    )
    .and_then(|v| v.l())
    .map_err(|e| {
        crate::error::AppError::Clipboard(format!("getSystemService: {}", e))
    })
}

fn call_method_ret<'a>(
    env: &mut jni::JNIEnv<'a>,
    obj: &JObject<'a>,
    name: &str,
    sig: &str,
    args: &[JValue<'a, 'a>],
) -> AppResult<JObject<'a>> {
    env.call_method(obj, name, sig, args)
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("JNI {} 失败: {}", name, e))
        })
}

fn read_item_text<'a>(env: &mut jni::JNIEnv<'a>, item: &JObject<'a>) -> AppResult<String> {
    let text = call_method_ret(
        env,
        item,
        "getText",
        "()Ljava/lang/CharSequence;",
        &[],
    )?;

    if text.is_null() {
        return Err(crate::error::AppError::Clipboard("无文本".into()));
    }

    let js = env
        .call_method(&text, "toString", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("toString: {}", e))
        })?;

    if js.is_null() {
        return Err(crate::error::AppError::Clipboard("toString 返回 null".into()));
    }

    unsafe {
        let jstr = jni::objects::JString::from_raw(js.into_raw());
        env.get_string(&jstr)
            .map(|s| s.into())
            .map_err(|e| {
                crate::error::AppError::Clipboard(format!("get_string: {}", e))
            })
    }
}

fn read_image_from_uri<'a>(
    env: &mut jni::JNIEnv<'a>,
    ctx: &JObject<'a>,
    uri: &JObject<'a>,
) -> AppResult<ClipboardContent> {
    let resolver = call_method_ret(
        env,
        ctx,
        "getContentResolver",
        "()Landroid/content/ContentResolver;",
        &[],
    )?;

    let stream = env
        .call_method(
            &resolver,
            "openInputStream",
            "(Landroid/net/Uri;)Ljava/io/InputStream;",
            &[JValue::Object(uri)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("openInputStream: {}", e))
        })?;

    if stream.is_null() {
        return Ok(ClipboardContent::None);
    }

    let bytes = read_input_stream_bytes(env, &stream)?;
    if bytes.is_empty() {
        return Ok(ClipboardContent::None);
    }

    if bytes.len() >= 4 && &bytes[..4] == b"\x89PNG" {
        return Ok(ClipboardContent::Image {
            width: 0,
            height: 0,
            data: bytes,
        });
    }

    decode_image_to_png(env, &bytes)
}

fn read_input_stream_bytes<'a>(env: &mut jni::JNIEnv<'a>, stream: &JObject<'a>) -> AppResult<Vec<u8>> {
    let baos = env
        .find_class("java/io/ByteArrayOutputStream")
        .and_then(|cls| env.new_object(&cls, "()V", &[]))
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("ByteArrayOutputStream: {}", e))
        })?;

    let buf = env.new_byte_array(8192).map_err(|e| {
        crate::error::AppError::Clipboard(format!("new_byte_array: {}", e))
    })?;

    loop {
        let n = env
            .call_method(stream, "read", "([B)I", &[JValue::Object(&buf)])
            .and_then(|v| v.i())
            .unwrap_or(-1);
        if n <= 0 {
            break;
        }
        env.call_method(
            &baos,
            "write",
            "([BII)V",
            &[JValue::Object(&buf), JValue::Int(0), JValue::Int(n)],
        )
        .ok();
    }

    let result = call_method_ret(env, &baos, "toByteArray", "()[B", &[])?;
    if result.is_null() {
        return Ok(Vec::new());
    }

    let arr = unsafe { jni::objects::JByteArray::from_raw(result.into_raw()) };
    let len = env.get_array_length(&arr).unwrap_or(0) as usize;
    if len == 0 {
        return Ok(Vec::new());
    }

    let elements = unsafe {
        env.get_array_elements(&arr, jni::objects::ReleaseMode::NoCopyBack)
    }
    .map_err(|e| {
        crate::error::AppError::Clipboard(format!("get_array_elements: {}", e))
    })?;

    let slice = unsafe { std::slice::from_raw_parts(elements.as_ptr() as *const u8, len) };
    Ok(slice.to_vec())
}

fn decode_image_to_png<'a>(env: &mut jni::JNIEnv<'a>, data: &[u8]) -> AppResult<ClipboardContent> {
    let arr = env
        .byte_array_from_slice(data)
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("byte_array_from_slice: {}", e))
        })?;

    let bf_cls = env
        .find_class("android/graphics/BitmapFactory")
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("BitmapFactory: {}", e))
        })?;

    let bitmap = env
        .call_static_method(
            &bf_cls,
            "decodeByteArray",
            "([BII)Landroid/graphics/Bitmap;",
            &[
                JValue::Object(&arr),
                JValue::Int(0),
                JValue::Int(data.len() as i32),
            ],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("decodeByteArray: {}", e))
        })?;

    if bitmap.is_null() {
        return Ok(ClipboardContent::None);
    }

    let baos = env
        .find_class("java/io/ByteArrayOutputStream")
        .and_then(|cls| env.new_object(&cls, "()V", &[]))
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("ByteArrayOutputStream: {}", e))
        })?;

    let cf_cls = env
        .find_class("android/graphics/Bitmap$CompressFormat")
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("CompressFormat: {}", e))
        })?;
    let png_format = env
        .get_static_field(&cf_cls, "PNG", "Landroid/graphics/Bitmap$CompressFormat;")
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("CompressFormat.PNG: {}", e))
        })?;

    env.call_method(
        &bitmap,
        "compress",
        "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z",
        &[
            JValue::Object(&png_format),
            JValue::Int(100),
            JValue::Object(&baos),
        ],
    )
    .map_err(|e| {
        crate::error::AppError::Clipboard(format!("compress: {}", e))
    })?;

    read_input_stream_bytes(env, &baos).map(|png_bytes| ClipboardContent::Image {
        width: 0,
        height: 0,
        data: png_bytes,
    })
}

fn write_image_to_clipboard<'a>(
    env: &mut jni::JNIEnv<'a>,
    ctx: &JObject<'a>,
    svc: &JObject<'a>,
    data: &[u8],
) -> AppResult<()> {
    let cache_dir = call_method_ret(env, ctx, "getCacheDir", "()Ljava/io/File;", &[])?;

    let abs_path = call_method_ret(env, &cache_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?;
    let path: String = if !abs_path.is_null() {
        unsafe {
            let js = jni::objects::JString::from_raw(abs_path.into_raw());
            env.get_string(&js).map(|s| s.into()).map_err(|e| {
                crate::error::AppError::Clipboard(format!("getString: {}", e))
            })?
        }
    } else {
        return Err(crate::error::AppError::Clipboard("getAbsolutePath 返回 null".into()));
    };

    let file_path = format!("{}/sharecopy_clip.png", path);
    let path_str = env.new_string(&file_path).map_err(|e| {
        crate::error::AppError::Clipboard(format!("new_string: {}", e))
    })?;

    let fos_cls = env.find_class("java/io/FileOutputStream").map_err(|e| {
        crate::error::AppError::Clipboard(format!("FileOutputStream: {}", e))
    })?;
    let fos = env
        .new_object(&fos_cls, "(Ljava/lang/String;)V", &[JValue::Object(&path_str)])
        .map_err(|e| crate::error::AppError::Clipboard(format!("FileOutputStream: {}", e)))?;

    let byte_arr = env.byte_array_from_slice(data).map_err(|e| {
        crate::error::AppError::Clipboard(format!("byte_array_from_slice: {}", e))
    })?;

    env.call_method(&fos, "write", "([B)V", &[JValue::Object(&byte_arr)])
        .map_err(|e| crate::error::AppError::Clipboard(format!("fos.write: {}", e)))?;
    env.call_method(&fos, "close", "()V", &[])
        .map_err(|e| crate::error::AppError::Clipboard(format!("fos.close: {}", e)))?;

    let file_cls = env.find_class("java/io/File").map_err(|e| {
        crate::error::AppError::Clipboard(format!("File: {}", e))
    })?;
    let file = env
        .new_object(&file_cls, "(Ljava/lang/String;)V", &[JValue::Object(&path_str)])
        .map_err(|e| crate::error::AppError::Clipboard(format!("new File: {}", e)))?;

    let uri_cls = env.find_class("android/net/Uri").map_err(|e| {
        crate::error::AppError::Clipboard(format!("Uri: {}", e))
    })?;

    let uri = env
        .call_static_method(
            &uri_cls,
            "fromFile",
            "(Ljava/io/File;)Landroid/net/Uri;",
            &[JValue::Object(&file)],
        )
        .and_then(|v| v.l())
        .map_err(|e| crate::error::AppError::Clipboard(format!("Uri.fromFile: {}", e)))?;

    let resolver = call_method_ret(
        env,
        ctx,
        "getContentResolver",
        "()Landroid/content/ContentResolver;",
        &[],
    )?;

    let clip_cls = env.find_class("android/content/ClipData").map_err(|e| {
        crate::error::AppError::Clipboard(format!("ClipData: {}", e))
    })?;
    let label = env.new_string("image").map_err(|e| {
        crate::error::AppError::Clipboard(format!("new_string: {}", e))
    })?;

    let clip = env
        .call_static_method(
            &clip_cls,
            "newUri",
            "(Landroid/content/ContentResolver;Ljava/lang/CharSequence;Landroid/net/Uri;)Landroid/content/ClipData;",
            &[
                JValue::Object(&resolver),
                JValue::Object(&label),
                JValue::Object(&uri),
            ],
        )
        .and_then(|v| v.l())
        .map_err(|e| crate::error::AppError::Clipboard(format!("ClipData.newUri: {}", e)))?;

    env.call_method(
        svc,
        "setPrimaryClip",
        "(Landroid/content/ClipData;)V",
        &[JValue::Object(&clip)],
    )
    .map_err(|e| crate::error::AppError::Clipboard(format!("setPrimaryClip: {}", e)))?;

    Ok(())
}
