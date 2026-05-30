//! Android 网络工具
//!
//! 通过 JNI 获取 WiFi IP 和设备名称，替代 if_addrs / hostname crate
//! （这两个 crate 在 Android 上行为不可靠）

use jni::objects::{JObject, JValue};
use jni::JavaVM;

use crate::error::AppResult;

/// 获取缓存的 JVM（与 clipboard_android 共享同一指针）
fn get_jvm() -> AppResult<JavaVM> {
    let ptr_val = crate::clipboard_android::GLOBAL_JVM_PTR.load(std::sync::atomic::Ordering::Acquire);
    if ptr_val == 0 {
        return Err(crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            "JVM 尚未初始化",
        )));
    }
    let ptr = ptr_val as *mut jni::sys::JavaVM;
    unsafe { JavaVM::from_raw(ptr) }.map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("无法从 raw pointer 创建 JavaVM: {}", e),
        ))
    })
}

/// 获取 Android WiFi IP（便捷函数，内部获取 JVM）
pub fn get_wifi_ip() -> AppResult<String> {
    get_wifi_ip_inner(&get_jvm()?)
}

/// 获取 Android 设备型号名称（便捷函数，内部获取 JVM）
pub fn get_device_model() -> AppResult<String> {
    get_device_model_inner(&get_jvm()?)
}

/// 获取 Android 设备 WiFi IP 地址（局域网 IP）
fn get_wifi_ip_inner(jvm: &JavaVM) -> AppResult<String> {
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("JNI 线程附加失败: {}", e),
        ))
    })?;

    // 获取 Application Context
    let context = get_app_context(&mut env)?;

    // 获取 WIFI_SERVICE
    let svc_name = env
        .get_static_field(
            "android/content/Context",
            "WIFI_SERVICE",
            "Ljava/lang/String;",
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("WIFI_SERVICE: {}", e),
            ))
        })?;

    let wifi_svc = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&svc_name)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("getSystemService(WIFI): {}", e),
            ))
        })?;

    if wifi_svc.is_null() {
        return Err(crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "WiFi 服务不可用",
        )));
    }

    // 调用 getConnectionInfo() 获取 WifiInfo
    let wifi_info = env
        .call_method(
            &wifi_svc,
            "getConnectionInfo",
            "()Landroid/net/wifi/WifiInfo;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("getConnectionInfo: {}", e),
            ))
        })?;

    if wifi_info.is_null() {
        return Err(crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "未连接到 WiFi",
        )));
    }

    // 获取 IP 地址（返回 int，需转换）
    let ip_int = env
        .call_method(&wifi_info, "getIpAddress", "()I", &[])
        .and_then(|v| v.i())
        .unwrap_or(0);

    if ip_int == 0 {
        return Err(crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            "WiFi IP 地址不可用",
        )));
    }

    // Android 的 getIpAddress() 返回的是小端序 int
    // 需要反转字节序得到标准 IP
    let ip = format!(
        "{}.{}.{}.{}",
        ip_int & 0xFF,
        (ip_int >> 8) & 0xFF,
        (ip_int >> 16) & 0xFF,
        (ip_int >> 24) & 0xFF
    );

    tracing::info!("Android WiFi IP: {}", ip);
    Ok(ip)
}

/// 获取 Android 设备型号名称（如 "Pixel 7", "Samsung Galaxy S24"）
fn get_device_model_inner(jvm: &JavaVM) -> AppResult<String> {
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("JNI 线程附加失败: {}", e),
        ))
    })?;

    let cls = env.find_class("android/os/Build").map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("find_class Build: {}", e),
        ))
    })?;

    let model = env
        .get_static_field(&cls, "MODEL", "Ljava/lang/String;")
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Build.MODEL: {}", e),
            ))
        })?;

    if model.is_null() {
        // 回退到 Build.MANUFACTURER + " " + Build.DEVICE
        let manufacturer = env
            .get_static_field(&cls, "MANUFACTURER", "Ljava/lang/String;")
            .and_then(|v| v.l())
            .ok();
        let device = env
            .get_static_field(&cls, "DEVICE", "Ljava/lang/String;")
            .and_then(|v| v.l())
            .ok();

        let name = match (manufacturer, device) {
            (Some(m), Some(d)) if !m.is_null() && !d.is_null() => {
                let m_str: String = unsafe {
                    let js = jni::objects::JString::from_raw(m.into_raw());
                    env.get_string(&js).map(|s| s.into()).unwrap_or_default()
                };
                let d_str: String = unsafe {
                    let js = jni::objects::JString::from_raw(d.into_raw());
                    env.get_string(&js).map(|s| s.into()).unwrap_or_default()
                };
                if m_str.is_empty() {
                    "Android 设备".to_string()
                } else {
                    format!("{} {}", m_str, d_str)
                }
            }
            _ => "Android 设备".to_string(),
        };
        return Ok(name);
    }

    unsafe {
        let js = jni::objects::JString::from_raw(model.into_raw());
        env.get_string(&js)
            .map(|s| s.into())
            .map_err(|e| {
                crate::error::AppError::Network(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("get_string Build.MODEL: {}", e),
                ))
            })
    }
}

/// 获取 WiFi Lock，防止屏幕关闭后 WiFi 进入省电模式导致入站连接断开
pub fn acquire_wifi_lock() -> AppResult<()> {
    let jvm = get_jvm()?;
    let mut env = jvm.attach_current_thread().map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("JNI 线程附加失败: {}", e),
        ))
    })?;

    let context = get_app_context(&mut env)?;

    let svc_name = env
        .get_static_field(
            "android/content/Context",
            "WIFI_SERVICE",
            "Ljava/lang/String;",
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("WIFI_SERVICE: {}", e),
            ))
        })?;

    let wifi_svc = env
        .call_method(
            &context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&svc_name)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("getSystemService(WIFI): {}", e),
            ))
        })?;

    if wifi_svc.is_null() {
        return Err(crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "WiFi 服务不可用",
        )));
    }

    // 创建 WifiLock: WIFI_MODE_FULL_HIGH_PERF = 3
    let tag = env.new_string("ShareCopy:WifiLock").map_err(|e| {
        crate::error::AppError::Network(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("new_string: {}", e),
        ))
    })?;

    let lock = env
        .call_method(
            &wifi_svc,
            "createWifiLock",
            "(ILjava/lang/String;)Landroid/net/wifi/WifiManager$WifiLock;",
            &[JValue::Int(3), JValue::Object(&tag)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("createWifiLock: {}", e),
            ))
        })?;

    if !lock.is_null() {
        env.call_method(&lock, "acquire", "()V", &[])
            .map_err(|e| {
                crate::error::AppError::Network(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("wifiLock.acquire: {}", e),
                ))
            })?;
        tracing::info!("WiFi Lock 已获取，防止 WiFi 休眠");
    }

    Ok(())
}

/// 获取 Android Application Context（内部使用）
fn get_app_context<'a>(env: &mut jni::JNIEnv<'a>) -> AppResult<JObject<'a>> {
    let ath_cls = env
        .find_class("android/app/ActivityThread")
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("find_class ActivityThread: {}", e),
            ))
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
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("currentActivityThread: {}", e),
            ))
        })?;

    let app = env
        .call_method(&ath, "getApplication", "()Landroid/app/Application;", &[])
        .and_then(|v| v.l())
        .map_err(|e| {
            crate::error::AppError::Network(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("getApplication: {}", e),
            ))
        })?;

    Ok(app)
}
