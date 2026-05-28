//! Android 剪贴板后端
//!
//! 通过 JNI 调用 Android ClipboardManager API。
//! Android 10+ 限制后台剪贴板访问，后台时需暂停轮询。

use jni::objects::{GlobalRef, JObject, JValue};
use jni::JavaVM;
use ndk_context;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::clipboard::{ClipboardBackend, ClipboardContent};
use crate::error::AppResult;

pub struct AndroidClipboardBackend {
    jvm: JavaVM,
    context: GlobalRef,
    last_hash: AtomicU64,
}

impl AndroidClipboardBackend {
    pub fn new() -> AppResult<Self> {
        let ctx = ndk_context::android_context();

        let jvm = unsafe {
            JavaVM::from_raw(ctx.vm() as *mut *const jni::sys::JNIInvokeInterface)
        }
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("获取 JavaVM 失败: {}", e))
        })?;

        let mut env = jvm.attach_current_thread().map_err(|e| {
            crate::error::AppError::Clipboard(format!("JNI attach 失败: {}", e))
        })?;

        let context_obj = unsafe { JObject::from_raw(ctx.context() as jni::sys::jobject) };
        let context_global = env.new_global_ref(context_obj).map_err(|e| {
            crate::error::AppError::Clipboard(format!("创建 GlobalRef 失败: {}", e))
        })?;

        Ok(Self {
            jvm,
            context: context_global,
            last_hash: AtomicU64::new(0),
        })
    }

    /// 在 JNI 环境中执行操作
    fn with_jni<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&mut jni::JNIEnv, &JObject, &JObject) -> AppResult<R>,
    {
        let mut env = self.jvm.attach_current_thread().map_err(|e| {
            crate::error::AppError::Clipboard(format!("JNI 线程附加失败: {}", e))
        })?;

        let ctx = self.context.as_obj();
        let svc = get_clipboard_service(&mut env, ctx)?;

        f(&mut env, ctx, &svc)
    }
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

            // 尝试读文本
            if let Ok(text_str) = read_item_text(env, &item) {
                if !text_str.is_empty() {
                    return Ok(ClipboardContent::Text(text_str));
                }
            }

            // 尝试读图片 URI
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
                        cls,
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
        // Android 无原生 change count，用内容哈希模拟
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
        .get_static_field(cls, "CLIPBOARD_SERVICE", "Ljava/lang/String;")
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

/// 调用返回对象的方法，简化返回值提取
fn call_method_ret<'a>(
    env: &mut jni::JNIEnv<'a>,
    obj: &JObject<'a>,
    name: &str,
    sig: &str,
    args: &[JValue<'a>],
) -> AppResult<JObject<'a>> {
    env.call_method(obj, name, sig, args)
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("JNI {} 失败: {}", name, e))
        })
}

fn read_item_text(env: &mut jni::JNIEnv, item: &JObject) -> AppResult<String> {
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

fn read_image_from_uri(
    env: &mut jni::JNIEnv,
    ctx: &JObject,
    uri: &JObject,
) -> AppResult<ClipboardContent> {
    let resolver = call_method_ret(
        env,
        ctx,
        "getContentResolver",
        "()Landroid/content/ContentResolver;",
        &[],
    )?;

    let stream = call_method_ret(
        env,
        &resolver,
        "openInputStream",
        "(Landroid/net/Uri;)Ljava/io/InputStream;",
        &[JValue::Object(uri)],
    )?;

    if stream.is_null() {
        return Ok(ClipboardContent::None);
    }

    let bytes = read_input_stream_bytes(env, &stream)?;
    if bytes.is_empty() {
        return Ok(ClipboardContent::None);
    }

    // 已是 PNG 则直接用
    if bytes.len() >= 4 && &bytes[..4] == b"\x89PNG" {
        return Ok(ClipboardContent::Image {
            width: 0,
            height: 0,
            data: bytes,
        });
    }

    // 否则用 BitmapFactory 转 PNG
    decode_image_to_png(env, &bytes)
}

fn read_input_stream_bytes(env: &mut jni::JNIEnv, stream: &JObject) -> AppResult<Vec<u8>> {
    let baos = env
        .find_class("java/io/ByteArrayOutputStream")
        .and_then(|cls| env.new_object(cls, "()V", &[]))
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
    let mut buf = vec![0u8; len];
    if len > 0 {
        env.get_byte_array_region(&arr, 0, &mut buf).map_err(|e| {
            crate::error::AppError::Clipboard(format!("get_byte_array_region: {}", e))
        })?;
    }
    Ok(buf)
}

fn decode_image_to_png(env: &mut jni::JNIEnv, data: &[u8]) -> AppResult<ClipboardContent> {
    let arr = env.byte_array_from_slice(data).map_err(|e| {
        crate::error::AppError::Clipboard(format!("byte_array_from_slice: {}", e))
    })?;

    let bf = call_method_ret(
        env,
        &env.find_class("android/graphics/BitmapFactory").map_err(|e| {
            crate::error::AppError::Clipboard(format!("BitmapFactory: {}", e))
        })?,
        "decodeByteArray",
        "([BII)Landroid/graphics/Bitmap;",
        &[JValue::Object(&arr), JValue::Int(0), JValue::Int(data.len() as i32)],
    )?;

    // call_method_ret 需要 obj 参数，但这是静态方法，用 JObject::null()
    let bf_cls = env.find_class("android/graphics/BitmapFactory").map_err(|e| {
        crate::error::AppError::Clipboard(format!("BitmapFactory: {}", e))
    })?;
    let bitmap = env
        .call_static_method(
            bf_cls,
            "decodeByteArray",
            "([BII)Landroid/graphics/Bitmap;",
            &[JValue::Object(&arr), JValue::Int(0), JValue::Int(data.len() as i32)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("decodeByteArray: {}", e))
        })?;

    if bitmap.is_null() {
        return Ok(ClipboardContent::None);
    }

    // bitmap.compress(Bitmap.CompressFormat.PNG, 100, baos)
    let baos = env
        .find_class("java/io/ByteArrayOutputStream")
        .and_then(|cls| env.new_object(cls, "()V", &[]))
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("ByteArrayOutputStream: {}", e))
        })?;

    let cf_cls = env
        .find_class("android/graphics/Bitmap$CompressFormat")
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("CompressFormat: {}", e))
        })?;
    let png_format = env
        .get_static_field(cf_cls, "PNG", "Landroid/graphics/Bitmap$CompressFormat;")
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("CompressFormat.PNG: {}", e))
        })?;

    env.call_method(
        &bitmap,
        "compress",
        "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z",
        &[JValue::Object(&png_format), JValue::Int(100), JValue::Object(&baos)],
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

fn write_image_to_clipboard(
    env: &mut jni::JNIEnv,
    ctx: &JObject,
    svc: &JObject,
    data: &[u8],
) -> AppResult<()> {
    // context.getCacheDir()
    let cache_dir = call_method_ret(env, ctx, "getCacheDir", "()Ljava/io/File;", &[])?;

    // getAbsolutePath()
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

    // 写入文件
    let fos = env
        .find_class("java/io/FileOutputStream")
        .and_then(|cls| env.new_object(cls, "(Ljava/lang/String;)V", &[JValue::Object(&path_str)]))
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("FileOutputStream: {}", e))
        })?;

    let byte_arr = env.byte_array_from_slice(data).map_err(|e| {
        crate::error::AppError::Clipboard(format!("byte_array_from_slice: {}", e))
    })?;

    env.call_method(&fos, "write", "([B)V", &[JValue::Object(&byte_arr)])
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("fos.write: {}", e))
        })?;
    env.call_method(&fos, "close", "()V", &[]).map_err(|e| {
        crate::error::AppError::Clipboard(format!("fos.close: {}", e))
    })?;

    // 创建 File 对象 -> Uri
    let file = env
        .find_class("java/io/File")
        .and_then(|cls| env.new_object(cls, "(Ljava/lang/String;)V", &[JValue::Object(&path_str)]))
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("new File: {}", e))
        })?;

    let uri_cls = env.find_class("android/net/Uri").map_err(|e| {
        crate::error::AppError::Clipboard(format!("Uri: {}", e))
    })?;

    let uri = env
        .call_static_method(
            uri_cls,
            "fromFile",
            "(Ljava/io/File;)Landroid/net/Uri;",
            &[JValue::Object(&file)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("Uri.fromFile: {}", e))
        })?;

    // ClipData.newUri(resolver, "image", uri)
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
            clip_cls,
            "newUri",
            "(Landroid/content/ContentResolver;Ljava/lang/CharSequence;Landroid/net/Uri;)Landroid/content/ClipData;",
            &[JValue::Object(&resolver), JValue::Object(&label), JValue::Object(&uri)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Clipboard(format!("ClipData.newUri: {}", e))
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
