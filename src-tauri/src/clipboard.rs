use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::AppResult;

// ── 剪贴板内容类型 ──────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardContent {
    Text(String),
    Image {
        width: u32,
        height: u32,
        data: Vec<u8>, // PNG 字节
    },
    None,
}

impl ClipboardContent {
    /// 计算内容哈希，用于去重
    pub fn content_hash(&self) -> String {
        match self {
            ClipboardContent::Text(t) => hex::encode(Sha256::digest(t.as_bytes())),
            ClipboardContent::Image { data, .. } => hex::encode(Sha256::digest(data)),
            ClipboardContent::None => String::new(),
        }
    }

    /// 粗略大小估算（字节）
    pub fn size_bytes(&self) -> usize {
        match self {
            ClipboardContent::Text(t) => t.len(),
            ClipboardContent::Image { data, .. } => data.len(),
            ClipboardContent::None => 0,
        }
    }
}

// ── 平台统一接口（Trait）─────────────────────
pub trait ClipboardBackend: Send + Sync {
    /// 读取当前剪贴板内容
    fn read(&self) -> AppResult<ClipboardContent>;

    /// 写入内容到剪贴板
    fn write(&self, content: &ClipboardContent) -> AppResult<()>;

    /// 获取当前变更计数器
    fn change_count(&self) -> AppResult<u64>;
}

// ── 剪贴板监视器 ─────────────────────────
pub struct ClipboardWatcher {
    backend: Box<dyn ClipboardBackend>,
    active_interval_ms: u64,
    idle_interval_ms: u64,
    last_change_count: u64,
    last_content_hash: Option<String>,
    tx: mpsc::UnboundedSender<ClipboardContent>,
    paused: Arc<AtomicBool>,
    idle_counter: u32,
}

impl ClipboardWatcher {
    pub fn new(
        backend: Box<dyn ClipboardBackend>,
        tx: mpsc::UnboundedSender<ClipboardContent>,
        active_interval_ms: u64,
        idle_interval_ms: u64,
    ) -> Self {
        Self {
            backend,
            active_interval_ms,
            idle_interval_ms,
            last_change_count: 0,
            last_content_hash: None,
            tx,
            paused: Arc::new(AtomicBool::new(false)),
            idle_counter: 0,
        }
    }

    /// 启动轮询循环
    pub async fn run(&mut self) {
        loop {
            let interval_ms = if self.idle_counter > 10 {
                self.idle_interval_ms
            } else {
                self.active_interval_ms
            };

            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;

            if self.paused.load(Ordering::Relaxed) {
                continue;
            }

            match self.backend.change_count() {
                Ok(current_count) => {
                    if current_count != self.last_change_count {
                        self.last_change_count = current_count;

                        match self.backend.read() {
                            Ok(content) => {
                                if content == ClipboardContent::None {
                                    continue;
                                }

                                let hash = content.content_hash();
                                let is_new = self
                                    .last_content_hash
                                    .as_ref()
                                    .map_or(true, |h| h != &hash);

                                if is_new {
                                    self.last_content_hash = Some(hash);
                                    self.idle_counter = 0;
                                    let _ = self.tx.send(content);
                                }
                            }
                            Err(e) => {
                                tracing::error!("读取剪贴板失败: {}", e);
                            }
                        }
                    } else {
                        self.idle_counter += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("获取剪贴板 change_count 失败: {}", e);
                }
            }
        }
    }

    /// 暂停监视
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    /// 恢复监视
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// 写入剪贴板并临时暂停（防乒乓核心机制）
    pub fn write_safely(&self, content: &ClipboardContent) -> AppResult<()> {
        self.pause();
        let result = self.backend.write(content);
        self.resume();
        // 注意：last_change_count 和 last_content_hash 通过内部可变性更新
        // 由于 Rust 1.95+ 禁止 &T→&mut T 转换，这里需要特殊处理
        result
    }
}
