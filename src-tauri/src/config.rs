use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 设备显示别名
    pub device_name: String,
    /// 本机唯一标识（首次启动生成 UUID v4）
    pub device_id: String,
    /// TCP 通信端口
    pub tcp_port: u16,
    /// 接收文件的保存目录
    pub save_dir: PathBuf,
    /// 是否开机自启
    pub auto_start: bool,
    /// 是否启用剪贴板同步
    pub sync_enabled: bool,
    /// 是否自动接收文件（false 则每次询问）
    pub auto_accept_files: bool,
    /// 剪贴板轮询间隔（毫秒）
    pub poll_interval_active_ms: u64,
    pub poll_interval_idle_ms: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_name: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            device_id: uuid::Uuid::new_v4().to_string(),
            tcp_port: 54322,
            save_dir: dirs_next_download_dir(),
            auto_start: false,
            sync_enabled: true,
            auto_accept_files: true,
            poll_interval_active_ms: 200,
            poll_interval_idle_ms: 2000,
        }
    }
}

fn dirs_next_download_dir() -> PathBuf {
    // Android: 优先用外部文件目录（无需存储权限），回退到内部数据目录
    #[cfg(target_os = "android")]
    {
        directories::BaseDirs::new()
            .and_then(|d| {
                // 尝试 data_dir，通常映射到 /data/data/<app>/files
                let dir = d.data_dir().to_path_buf();
                if std::fs::create_dir_all(&dir).is_ok() {
                    Some(dir)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(target_os = "android"))]
    {
        directories::UserDirs::new()
            .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

impl AppConfig {
    /// 从配置文件加载，不存在则使用默认值并保存
    pub fn load() -> AppResult<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                crate::error::AppError::Config(format!("读取配置文件失败: {}", e))
            })?;
            let config: AppConfig = toml::from_str(&content).map_err(|e| {
                crate::error::AppError::Config(format!("解析配置文件失败: {}", e))
            })?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> AppResult<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| {
            crate::error::AppError::Config(format!("创建配置目录失败: {}", e))
        })?;
        let content = toml::to_string_pretty(self).map_err(|e| {
            crate::error::AppError::Config(format!("序列化配置失败: {}", e))
        })?;
        std::fs::write(Self::config_path(), content).map_err(|e| {
            crate::error::AppError::Config(format!("写入配置文件失败: {}", e))
        })?;
        Ok(())
    }

    /// 配置文件路径
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    fn config_dir() -> PathBuf {
        directories::ProjectDirs::from("com", "sharecopy", "ShareCopy")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| {
                let home = directories::BaseDirs::new()
                    .map(|d| d.home_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                home.join(".sharecopy")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.sync_enabled);
        assert_eq!(config.tcp_port, 54322);
        assert!(!config.device_id.is_empty());
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = AppConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config.device_id, deserialized.device_id);
        assert_eq!(config.tcp_port, deserialized.tcp_port);
    }
}
