use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::network::NetworkManager;
use crate::protocol::{
    FileDataChunkPayload, FileTransferReqPayload, Message,
};

pub const CHUNK_SIZE: usize = 64 * 1024; // 64KB

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
    transfer_id: String,
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

        // 计算完整文件 SHA256
        let file_data = tokio::fs::read(file_path).await.map_err(|e| {
            AppError::Transfer(format!("读取文件失败: {}", e))
        })?;

        use sha2::{Digest, Sha256};
        let sha256 = hex::encode(Sha256::digest(&file_data));

        // 发送文件传输请求
        let req = Message::FileTransferReq(FileTransferReqPayload {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            file_size,
            total_chunks,
            sha256: sha256.clone(),
        });

        self.network.send(target_device_id, &req)?;

        // 发送进度
        let _ = self.progress_tx.send(TransferProgress {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            file_size,
            bytes_transferred: 0,
            progress: 0.0,
            state: TransferState::Transferring,
            error: None,
            save_path: None,
        });

        // 分块发送
        let network = self.network.clone();
        let progress_tx = self.progress_tx.clone();
        let tid = transfer_id.clone();
        let fname = file_name.clone();
        let target_id = target_device_id.to_string();

        tokio::spawn(async move {
            for i in 0..total_chunks {
                let start = i as usize * CHUNK_SIZE;
                let end = std::cmp::min(start + CHUNK_SIZE, file_data.len());
                let chunk = file_data[start..end].to_vec();
                let chunk_sha256 = hex::encode(Sha256::digest(&chunk));

                let msg = Message::FileDataChunk(FileDataChunkPayload {
                    transfer_id: tid.clone(),
                    chunk_index: i,
                    total_chunks,
                    data: chunk,
                    sha256_chunk: chunk_sha256,
                });

                if let Err(e) = network.send(&target_id, &msg) {
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
                    });
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
                });
            }
        });

        Ok(transfer_id)
    }

    /// 处理文件传输请求
    pub fn handle_file_request(&self, payload: &FileTransferReqPayload) {
        let transfer = IncomingTransfer {
            transfer_id: payload.transfer_id.clone(),
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
        });
    }

    /// 处理文件数据块
    pub fn handle_file_chunk(&self, payload: &FileDataChunkPayload) -> AppResult<()> {
        use sha2::{Digest, Sha256};

        let mut incoming = self.incoming.lock();
        let transfer = incoming
            .get_mut(&payload.transfer_id)
            .ok_or_else(|| AppError::Transfer("未知传输会话".into()))?;

        // 校验块 SHA256
        let chunk_hash = hex::encode(Sha256::digest(&payload.data));
        if chunk_hash != payload.sha256_chunk {
            return Err(AppError::ChecksumMismatch {
                expected: payload.sha256_chunk.clone(),
                actual: chunk_hash,
            });
        }

        transfer
            .chunks
            .insert(payload.chunk_index, payload.data.clone());

        let received = transfer.chunks.len() as u64;
        let progress = received as f64 / transfer.total_chunks as f64;

        let _ = self.progress_tx.send(TransferProgress {
            transfer_id: payload.transfer_id.clone(),
            file_name: transfer.file_name.clone(),
            file_size: transfer.file_size,
            bytes_transferred: received * CHUNK_SIZE as u64,
            progress,
            state: TransferState::Transferring,
            error: None,
            save_path: None,
        });

        // 检查是否集齐所有块
        if transfer.chunks.len() == transfer.total_chunks as usize {
            let transfer = incoming.remove(&payload.transfer_id).unwrap();
            drop(incoming);

            let save_dir = self.save_dir.clone();
            let progress_tx = self.progress_tx.clone();
            let transfer_id = payload.transfer_id.clone();
            let file_name = transfer.file_name.clone();

            tokio::spawn(async move {
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
