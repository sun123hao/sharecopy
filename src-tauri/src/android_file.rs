//! Android 文件系统工具
//!
//! 处理 content:// URI 转换、目录选择等 Android 特有逻辑

use jni::objects::{JObject, JValue};
use jni::JavaVM;

use crate::error::AppResult;

/// 将 Android content:// URI 的内容复制到临时文件，返回临时文件路径
/// 桌面端直接返回原路径
pub fn resolve_content_uri(uri: &str) -> AppResult<String> {
    #[cfg(target_os = "android")]
    {
        if !uri.starts_with("content://") {
            return Ok(uri.to_string());
        }
        resolve_content_uri_android(uri)
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(uri.to_string())
    }
}

#[cfg(target_os = "android")]
fn resolve_content_uri_android(uri: &str) -> AppResult<String> {
    let jvm = get_jvm()?;
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Transfer(format!("JNI attach 失败: {}", e))
    })?;

    let context = get_app_context(&mut env)?;

    // 获取 ContentResolver
    let resolver = env
        .call_method(
            &context,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getContentResolver: {}", e))
        })?;

    // 解析 URI
    let uri_cls = env.find_class("android/net/Uri").map_err(|e| {
        crate::error::AppError::Transfer(format!("Uri: {}", e))
    })?;
    let uri_str = env.new_string(uri).map_err(|e| {
        crate::error::AppError::Transfer(format!("new_string: {}", e))
    })?;
    let uri_obj = env
        .call_static_method(
            &uri_cls,
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_str)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("Uri.parse: {}", e))
        })?;

    // 获取文件名
    let file_name = get_file_name_from_uri(&mut env, &resolver, &uri_obj)
        .unwrap_or_else(|_| "shared_file".to_string());

    // 打开 InputStream
    let input_stream = env
        .call_method(
            &resolver,
            "openInputStream",
            "(Landroid/net/Uri;)Ljava/io/InputStream;",
            &[JValue::Object(&uri_obj)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("openInputStream: {}", e))
        })?;

    if input_stream.is_null() {
        return Err(crate::error::AppError::Transfer(
            "无法打开文件流".into(),
        ));
    }

    // 复制到临时文件
    let cache_dir = env
        .call_method(&context, "getCacheDir", "()Ljava/io/File;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getCacheDir: {}", e))
        })?;

    let abs_path = env
        .call_method(&cache_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getAbsolutePath: {}", e))
        })?;
    let cache_path: String = unsafe {
        let js = jni::objects::JString::from_raw(abs_path.into_raw());
        env.get_string(&js)
            .map(|s| s.into())
            .map_err(|e| {
                crate::error::AppError::Transfer(format!("get_string: {}", e))
            })?
    };

    let tmp_path = format!("{}/sharecopy_{}", cache_path, file_name);
    let tmp_path_str = env.new_string(&tmp_path).map_err(|e| {
        crate::error::AppError::Transfer(format!("new_string: {}", e))
    })?;

    // 创建 FileOutputStream
    let fos_cls = env.find_class("java/io/FileOutputStream").map_err(|e| {
        crate::error::AppError::Transfer(format!("FileOutputStream: {}", e))
    })?;
    let fos = env
        .new_object(&fos_cls, "(Ljava/lang/String;)V", &[JValue::Object(&tmp_path_str)])
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("new FileOutputStream: {}", e))
        })?;

    // 拷贝字节
    let buf = env.new_byte_array(65536).map_err(|e| {
        crate::error::AppError::Transfer(format!("new_byte_array: {}", e))
    })?;
    loop {
        let n = env
            .call_method(&input_stream, "read", "([B)I", &[JValue::Object(&buf)])
            .and_then(|v| v.i())
            .unwrap_or(-1);
        if n <= 0 {
            break;
        }
        env.call_method(
            &fos,
            "write",
            "([BII)V",
            &[JValue::Object(&buf), JValue::Int(0), JValue::Int(n)],
        )
        .ok();
    }
    env.call_method(&input_stream, "close", "()V", &[]).ok();
    env.call_method(&fos, "close", "()V", &[]).ok();

    tracing::info!("content URI 已复制到: {}", tmp_path);
    Ok(tmp_path)
}

/// 从 URI 获取文件名
#[cfg(target_os = "android")]
fn get_file_name_from_uri<'a>(
    env: &mut jni::JNIEnv<'a>,
    resolver: &JObject<'a>,
    uri: &JObject<'a>,
) -> AppResult<String> {
    // 尝试通过 Cursor 查询 DISPLAY_NAME
    let cursor = env
        .call_method(
            resolver,
            "query",
            "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
            &[
                JValue::Object(uri),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
                JValue::Object(&JObject::null()),
            ],
        )
        .and_then(|v| v.l())
        .ok();

    if let Some(cursor) = cursor {
        if !cursor.is_null() {
            let display_name_idx = env
                .call_method(&cursor, "getColumnIndex", "(Ljava/lang/String;)I", &[
                    JValue::Object(&env.new_string("_display_name").unwrap()),
                ])
                .and_then(|v| v.i())
                .unwrap_or(-1);

            if display_name_idx >= 0 {
                let moved = env
                    .call_method(&cursor, "moveToFirst", "()Z", &[])
                    .and_then(|v| v.z())
                    .unwrap_or(false);
                if moved {
                    let name = env
                        .call_method(&cursor, "getString", "(I)Ljava/lang/String;", &[
                            JValue::Int(display_name_idx),
                        ])
                        .and_then(|v| v.l())
                        .ok();
                    if let Some(name) = name {
                        if !name.is_null() {
                            let result: String = unsafe {
                                let js = jni::objects::JString::from_raw(name.into_raw());
                                env.get_string(&js).map(|s| s.into()).unwrap_or_default()
                            };
                            env.call_method(&cursor, "close", "()V", &[]).ok();
                            if !result.is_empty() {
                                return Ok(result);
                            }
                        }
                    }
                }
            }
            env.call_method(&cursor, "close", "()V", &[]).ok();
        }
    }

    // 回退：从 URI 的最后一个路径段提取文件名
    let uri_str = env
        .call_method(uri, "toString", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .ok();
    if let Some(uri_str) = uri_str {
        if !uri_str.is_null() {
            let s: String = unsafe {
                let js = jni::objects::JString::from_raw(uri_str.into_raw());
                env.get_string(&js).map(|s| s.into()).unwrap_or_default()
            };
            if let Some(name) = s.split('/').last() {
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Ok("file.bin".to_string())
}

/// 获取 Android 缓存 JVM
#[cfg(target_os = "android")]
fn get_jvm() -> AppResult<JavaVM> {
    let ptr_val =
        crate::clipboard_android::GLOBAL_JVM_PTR.load(std::sync::atomic::Ordering::Acquire);
    if ptr_val == 0 {
        return Err(crate::error::AppError::Transfer("JVM 尚未初始化".into()));
    }
    let ptr = ptr_val as *mut jni::sys::JavaVM;
    unsafe { JavaVM::from_raw(ptr) }.map_err(|e| {
        crate::error::AppError::Transfer(format!("无法创建 JavaVM: {}", e))
    })
}

/// 获取 Android Application Context
#[cfg(target_os = "android")]
fn get_app_context<'a>(env: &mut jni::JNIEnv<'a>) -> AppResult<JObject<'a>> {
    let ath_cls = env
        .find_class("android/app/ActivityThread")
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("ActivityThread: {}", e))
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
            crate::error::AppError::Transfer(format!("currentActivityThread: {}", e))
        })?;

    let app = env
        .call_method(&ath, "getApplication", "()Landroid/app/Application;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getApplication: {}", e))
        })?;

    Ok(app)
}

/// 获取 Android 默认保存目录（外部文件目录，用户可访问）
#[cfg(target_os = "android")]
pub fn get_default_save_dir() -> AppResult<std::path::PathBuf> {
    let jvm = get_jvm()?;
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Transfer(format!("JNI attach: {}", e))
    })?;

    let context = get_app_context(&mut env)?;
    let ext_files = env
        .call_method(&context, "getExternalFilesDir", "(Ljava/lang/String;)Ljava/io/File;", &[
            JValue::Object(&JObject::null()),
        ])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getExternalFilesDir: {}", e))
        })?;

    if ext_files.is_null() {
        return Err(crate::error::AppError::Transfer("外部文件目录为空".into()));
    }

    let abs = env
        .call_method(&ext_files, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Transfer(format!("getAbsolutePath: {}", e))
        })?;

    let path: String = unsafe {
        let js = jni::objects::JString::from_raw(abs.into_raw());
        env.get_string(&js).map(|s| s.into()).map_err(|e| {
            crate::error::AppError::Transfer(format!("get_string: {}", e))
        })?
    };

    Ok(std::path::PathBuf::from(path))
}

/// 获取 Android 可用的保存目录列表
/// 返回应用外部文件目录和标准公共目录
#[cfg(target_os = "android")]
pub fn get_save_directories() -> AppResult<Vec<String>> {
    let jvm = get_jvm()?;
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Transfer(format!("JNI attach: {}", e))
    })?;

    let context = get_app_context(&mut env)?;
    let mut dirs = Vec::new();

    // 1. 应用外部文件目录（始终可写）
    let ext_files = env
        .call_method(&context, "getExternalFilesDir", "(Ljava/lang/String;)Ljava/io/File;", &[
            JValue::Object(&JObject::null()),
        ])
        .and_then(|v| v.l())
        .ok();
    if let Some(dir) = ext_files {
        if !dir.is_null() {
            let abs = env
                .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
                .and_then(|v| v.l())
                .ok();
            if let Some(abs) = abs {
                if !abs.is_null() {
                    let s: String = unsafe {
                        let js = jni::objects::JString::from_raw(abs.into_raw());
                        env.get_string(&js).map(|s| s.into()).unwrap_or_default()
                    };
                    if !s.is_empty() {
                        dirs.push(s);
                    }
                }
            }
        }
    }

    // 2. Environment.getExternalStoragePublicDirectory(DIRECTORY_DOWNLOADS)
    let env_cls = env.find_class("android/os/Environment").ok();
    if let Some(env_cls) = env_cls {
        let downloads = env
            .get_static_field(&env_cls, "DIRECTORY_DOWNLOADS", "Ljava/lang/String;")
            .and_then(|v| v.l())
            .ok();
        if let Some(downloads) = downloads {
            let public_dir = env
                .call_static_method(
                    &env_cls,
                    "getExternalStoragePublicDirectory",
                    "(Ljava/lang/String;)Ljava/io/File;",
                    &[JValue::Object(&downloads)],
                )
                .and_then(|v| v.l())
                .ok();
            if let Some(public_dir) = public_dir {
                if !public_dir.is_null() {
                    let abs = env
                        .call_method(&public_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
                        .and_then(|v| v.l())
                        .ok();
                    if let Some(abs) = abs {
                        if !abs.is_null() {
                            let s: String = unsafe {
                                let js = jni::objects::JString::from_raw(abs.into_raw());
                                env.get_string(&js).map(|s| s.into()).unwrap_or_default()
                            };
                            if !s.is_empty() && !dirs.contains(&s) {
                                dirs.push(s);
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::info!("Android 保存目录: {:?}", dirs);
    Ok(dirs)
}
