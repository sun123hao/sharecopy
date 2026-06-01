use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::network::NetworkManager;
use crate::protocol::{
    FileDataChunkPayload, FileTransferReqPayload, Message,
};

pub const CHUNK_SIZE: usize = 256 * 1024; // 256KB

// ── 传输进度 ──────────────────────────────
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub progress: f64, // 0.0 ~ 1.0
    pub state: TransferState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_path: Option<String>,
    /// 关联的设备 ID（发送端为目标设备，接收端为来源设备）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// 传输事件时间戳（Unix 毫秒）
    pub timestamp: u64,
}

impl TransferProgress {
    /// 创建初始进度（0% Transferring），调用处用结构体更新语法覆写变化的字段
    fn initial(transfer_id: String, file_name: String, file_size: u64) -> Self {
        Self {
            transfer_id,
            file_name,
            file_size,
            bytes_transferred: 0,
            progress: 0.0,
            state: TransferState::Transferring,
            error: None,
            save_path: None,
            device_id: None,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferState {
    Pending,
    Transferring,
    Completed,
    Failed,
}

// ── 进行中的传输（接收端）────────────────────
struct IncomingTransfer {
    _transfer_id: String,
    source_device_id: String,
    file_name: String,
    file_size: u64,
    total_chunks: u32,
    chunks: HashMap<u32, Vec<u8>>,
    sha256: String,
}

// ── 文件传输管理器 ──────────────────────────
pub struct FileTransferManager {
    save_dir: PathBuf,
    network: Arc<NetworkManager>,
    incoming: parking_lot::Mutex<HashMap<String, IncomingTransfer>>,
    progress_tx: mpsc::UnboundedSender<TransferProgress>,
    /// 活跃发送任务的取消令牌：transfer_id → cancel_flag
    cancel_tokens: Arc<dashmap::DashMap<String, Arc<AtomicBool>>>,
}

impl FileTransferManager {
    pub fn new(
        save_dir: PathBuf,
        network: Arc<NetworkManager>,
        progress_tx: mpsc::UnboundedSender<TransferProgress>,
    ) -> Self {
        Self {
            save_dir,
            network,
            incoming: parking_lot::Mutex::new(HashMap::new()),
            progress_tx,
            cancel_tokens: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// 发送文件到指定设备
    pub async fn send_file(
        &self,
        file_path: &Path,
        target_device_id: &str,
    ) -> AppResult<String> {
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let metadata = tokio::fs::metadata(file_path).await.map_err(|e| {
            AppError::Transfer(format!("无法读取文件: {}", e))
        })?;

        let file_size = metadata.len();
        let total_chunks =
            ((file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64) as u32;

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let target_id_owned = target_device_id.to_string();

        // 注册取消令牌
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_tokens.insert(transfer_id.clone(), cancel.clone());

        // 立即发送初始进度，让前端立刻显示传输文件名和 0% 进度
        let _ = self.progress_tx.send(TransferProgress {
            device_id: Some(target_id_owned.clone()),
            ..TransferProgress::initial(
                transfer_id.clone(),
                file_name.clone(),
                file_size,
            )
        });

        // 读取文件 + 计算完整文件 SHA256
        let file_data = match tokio::fs::read(file_path).await {
            Ok(d) => d,
            Err(e) => {
                // 文件读取失败时发送 Failed 事件清理前端幽灵进度条目
                self.cancel_tokens.remove(&transfer_id);
                let _ = self.progress_tx.send(TransferProgress {
                    state: TransferState::Failed,
                    error: Some(format!("读取文件失败: {}", e)),
                    device_id: Some(target_id_owned.clone()),
                    ..TransferProgress::initial(transfer_id.clone(), file_name.clone(), file_size)
                });
                return Err(AppError::Transfer(format!("读取文件失败: {}", e)));
            }
        };

        use sha2::{Digest, Sha256};
        let sha256 = hex::encode(Sha256::digest(&file_data));

        // 发送文件传输请求（接收端据此创建接收会话）
        let req = Message::FileTransferReq(FileTransferReqPayload {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            file_size,
            total_chunks,
            sha256: sha256.clone(),
        });

        // 发送文件传输请求（接收端据此创建接收会话）
        if let Err(e) = self.network.send(&target_id_owned, &req) {
            // 网络发送失败时发送 Failed 事件清理前端幽灵进度条目
            self.cancel_tokens.remove(&transfer_id);
            let _ = self.progress_tx.send(TransferProgress {
                state: TransferState::Failed,
                error: Some(format!("发送失败: {}", e)),
                device_id: Some(target_id_owned.clone()),
                ..TransferProgress::initial(transfer_id.clone(), file_name.clone(), file_size)
            });
            return Err(e);
        }

        // 分块发送：spawn_blocking 上编码+推送，避免异步调度间隙
        let network = self.network.clone();
        let progress_tx = self.progress_tx.clone();
        let cancel_tokens = self.cancel_tokens.clone();
        let tid = transfer_id.clone();
        let fname = file_name.clone();
        let tid_for_cleanup = transfer_id.clone();
        let tid_for_cancel = transfer_id.clone();

        tokio::task::spawn_blocking(move || {
            for i in 0..total_chunks {
                // 每块发送前检查取消令牌
                if cancel.load(Ordering::Relaxed) {
                    let _ = progress_tx.send(TransferProgress {
                        state: TransferState::Failed,
                        error: Some("传输已取消".into()),
                        device_id: Some(target_id_owned.clone()),
                        ..TransferProgress::initial(tid.clone(), fname.clone(), file_size)
                    });
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }

                let start = i as usize * CHUNK_SIZE;
                let end = std::cmp::min(start + CHUNK_SIZE, file_data.len());

                let msg = Message::FileDataChunk(FileDataChunkPayload {
                    transfer_id: tid.clone(),
                    chunk_index: i,
                    total_chunks,
                    data: file_data[start..end].to_vec(),
                    sha256_chunk: String::new(),
                });

                let encoded = match msg.encode() {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::error!("编码块 {} 失败: {}", i, e);
                        cancel_tokens.remove(&tid_for_cancel);
                        return;
                    }
                };

                if let Err(e) = network.send_raw(&target_id_owned, encoded) {
                    tracing::error!("发送文件块 {} 失败: {}", i, e);
                    let _ = progress_tx.send(TransferProgress {
                        transfer_id: tid.clone(),
                        file_name: fname.clone(),
                        file_size,
                        bytes_transferred: end as u64,
                        progress: end as f64 / file_size as f64,
                        state: TransferState::Failed,
                        error: Some(format!("发送失败: {}", e)),
                        save_path: None,
                        device_id: Some(target_id_owned.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    });
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }

                let progress = end as f64 / file_size as f64;
                let _ = progress_tx.send(TransferProgress {
                    transfer_id: tid.clone(),
                    file_name: fname.clone(),
                    file_size,
                    bytes_transferred: end as u64,
                    progress,
                    state: if i + 1 == total_chunks {
                        TransferState::Completed
                    } else {
                        TransferState::Transferring
                    },
                    error: None,
                    save_path: None,
                    device_id: Some(target_id_owned.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
            }
            // file_data 在此 drop，释放内存
            drop(file_data);
            cancel_tokens.remove(&tid_for_cleanup);
        });

        Ok(transfer_id)
    }

    /// 处理文件传输请求
    pub fn handle_file_request(&self, payload: &FileTransferReqPayload, source_device_id: &str) {
        let transfer = IncomingTransfer {
            _transfer_id: payload.transfer_id.clone(),
            source_device_id: source_device_id.to_string(),
            file_name: payload.file_name.clone(),
            file_size: payload.file_size,
            total_chunks: payload.total_chunks,
            chunks: HashMap::new(),
            sha256: payload.sha256.clone(),
        };

        self.incoming
            .lock()
            .insert(payload.transfer_id.clone(), transfer);

        let _ = self.progress_tx.send(TransferProgress {
            transfer_id: payload.transfer_id.clone(),
            file_name: payload.file_name.clone(),
            file_size: payload.file_size,
            bytes_transferred: 0,
            progress: 0.0,
            state: TransferState::Pending,
            error: None,
            save_path: None,
            device_id: Some(source_device_id.to_string()),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    /// 取消传输（发送端调用）
    pub fn cancel_transfer(&self, transfer_id: &str) -> AppResult<()> {
        // 1. 设置取消令牌（停止 spawn_blocking 循环）
        if let Some(token) = self.cancel_tokens.get(transfer_id) {
            token.store(true, Ordering::Relaxed);
        }

        // 2. 清理接收端会话（如果我们是接收方）
        if let Some(removed) = self.incoming.lock().remove(transfer_id) {
            let _ = self.progress_tx.send(TransferProgress {
                transfer_id: transfer_id.to_string(),
                file_name: removed.file_name.clone(),
                file_size: removed.file_size,
                bytes_transferred: 0,
                progress: 0.0,
                state: TransferState::Failed,
                error: Some("传输已取消".into()),
                save_path: None,
                device_id: Some(removed.source_device_id),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });
        }

        Ok(())
    }

    /// 处理远端发来的取消消息
    pub fn handle_transfer_cancel(&self, transfer_id: &str) {
        if let Some(removed) = self.incoming.lock().remove(transfer_id) {
            let _ = self.progress_tx.send(TransferProgress {
                transfer_id: transfer_id.to_string(),
                file_name: removed.file_name.clone(),
                file_size: removed.file_size,
                bytes_transferred: 0,
                progress: 0.0,
                state: TransferState::Failed,
                error: Some("发送方取消了传输".into()),
                save_path: None,
                device_id: Some(removed.source_device_id),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });
        }
    }

    /// 处理文件数据块
    pub fn handle_file_chunk(&self, payload: &FileDataChunkPayload) -> AppResult<()> {
        let mut incoming = self.incoming.lock();
        let transfer = incoming
            .get_mut(&payload.transfer_id)
            .ok_or_else(|| AppError::Transfer("未知传输会话".into()))?;

        // 逐块 SHA256 已跳过（TCP 层保证完整性，最终文件 SHA256 做端到端校验）
        // 仅在发送方提供了非空 hash 时才验证（兼容旧版本）
        if !payload.sha256_chunk.is_empty() {
            use sha2::{Digest, Sha256};
            let chunk_hash = hex::encode(Sha256::digest(&payload.data));
            if chunk_hash != payload.sha256_chunk {
                return Err(AppError::ChecksumMismatch {
                    expected: payload.sha256_chunk.clone(),
                    actual: chunk_hash,
                });
            }
        }

        transfer
            .chunks
            .insert(payload.chunk_index, payload.data.clone());

        let received = transfer.chunks.len() as u64;
        let progress = received as f64 / transfer.total_chunks as f64;

        let src_id = transfer.source_device_id.clone();
        let _ = self.progress_tx.send(TransferProgress {
            transfer_id: payload.transfer_id.clone(),
            file_name: transfer.file_name.clone(),
            file_size: transfer.file_size,
            bytes_transferred: received * CHUNK_SIZE as u64,
            progress,
            state: TransferState::Transferring,
            error: None,
            save_path: None,
            device_id: Some(src_id),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });
        if transfer.chunks.len() == transfer.total_chunks as usize {
            let transfer = incoming.remove(&payload.transfer_id).unwrap();
            let src_device_id = transfer.source_device_id.clone();
            drop(incoming);

            let save_dir = self.save_dir.clone();
            let progress_tx = self.progress_tx.clone();
            let transfer_id = payload.transfer_id.clone();
            let file_name = transfer.file_name.clone();

            tokio::spawn(async move {
                use sha2::{Digest, Sha256};
                // 组装文件
                let mut data: Vec<u8> = Vec::with_capacity(transfer.file_size as usize);
                for i in 0..transfer.total_chunks {
                    if let Some(chunk) = transfer.chunks.get(&i) {
                        data.extend_from_slice(chunk);
                    }
                }

                // 校验完整文件 SHA256
                let file_hash = hex::encode(Sha256::digest(&data));
                if file_hash != transfer.sha256 {
                    let _ = progress_tx.send(TransferProgress {
                        transfer_id: transfer_id.clone(),
                        file_name: file_name.clone(),
                        file_size: transfer.file_size,
                        bytes_transferred: data.len() as u64,
                        progress: 1.0,
                        state: TransferState::Failed,
                        error: Some("文件校验失败".into()),
                        save_path: None,
                        device_id: Some(src_device_id.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    });
                    return;
                }

                // 保存文件
                let save_path = save_dir.join(&file_name);
                if let Err(e) = tokio::fs::write(&save_path, &data).await {
                    let _ = progress_tx.send(TransferProgress {
                        transfer_id: transfer_id.clone(),
                        file_name: file_name.clone(),
                        file_size: transfer.file_size,
                        bytes_transferred: data.len() as u64,
                        progress: 1.0,
                        state: TransferState::Failed,
                        error: Some(format!("保存失败: {}", e)),
                        save_path: None,
                        device_id: Some(src_device_id.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    });
                    return;
                }

                let _ = progress_tx.send(TransferProgress {
                    transfer_id,
                    file_name,
                    file_size: transfer.file_size,
                    bytes_transferred: data.len() as u64,
                    progress: 1.0,
                    state: TransferState::Completed,
                    error: None,
                    save_path: Some(save_path.to_string_lossy().to_string()),
                    device_id: Some(src_device_id),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
            });
        }

        Ok(())
    }

    /// 获取默认保存目录
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }
}
