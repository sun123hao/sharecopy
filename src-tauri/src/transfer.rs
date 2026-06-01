use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::network::NetworkManager;
use crate::protocol::{
    FileDataChunkPayload, FileTransferReqPayload, Message,
};

pub const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB

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
    received_count: u32,
}

// ── 辅助：发送失败进度事件 ────────────────────
fn send_failed(
    tx: &mpsc::UnboundedSender<TransferProgress>,
    tid: &str, fname: &str, file_size: u64, error: &str,
    device_id: Option<String>,
) {
    let _ = tx.send(TransferProgress {
        transfer_id: tid.to_string(),
        file_name: fname.to_string(),
        file_size,
        bytes_transferred: 0,
        progress: 0.0,
        state: TransferState::Failed,
        error: Some(error.to_string()),
        save_path: None,
        device_id,
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
    });
}

// ── 文件传输管理器 ──────────────────────────
pub struct FileTransferManager {
    save_dir: PathBuf,
    network: Arc<NetworkManager>,
    incoming: parking_lot::Mutex<HashMap<String, IncomingTransfer>>,
    progress_tx: mpsc::UnboundedSender<TransferProgress>,
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

    /// 发送文件到指定设备（流式边读边发）
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
        let total_chunks = if file_size == 0 {
            0
        } else {
            ((file_size + CHUNK_SIZE as u64 - 1) / CHUNK_SIZE as u64) as u32
        };

        let transfer_id = uuid::Uuid::new_v4().to_string();
        let target_id_owned = target_device_id.to_string();

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_tokens.insert(transfer_id.clone(), cancel.clone());

        // 空文件：直接标记完成
        if file_size == 0 {
            let _ = self.progress_tx.send(TransferProgress {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                file_size: 0,
                bytes_transferred: 0,
                progress: 1.0,
                state: TransferState::Completed,
                error: None,
                save_path: None,
                device_id: Some(target_id_owned.clone()),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });
            self.cancel_tokens.remove(&transfer_id);
            return Ok(transfer_id);
        }

        // 发送初始进度
        let _ = self.progress_tx.send(TransferProgress {
            device_id: Some(target_id_owned.clone()),
            ..TransferProgress::initial(
                transfer_id.clone(),
                file_name.clone(),
                file_size,
            )
        });

        // 发送文件传输请求
        let req = Message::FileTransferReq(FileTransferReqPayload {
            transfer_id: transfer_id.clone(),
            file_name: file_name.clone(),
            file_size,
            total_chunks,
            sha256: String::new(),
        });

        if let Err(e) = self.network.send(&target_id_owned, &req) {
            self.cancel_tokens.remove(&transfer_id);
            send_failed(&self.progress_tx, &transfer_id, &file_name, file_size,
                &format!("发送失败: {}", e), Some(target_id_owned.clone()));
            return Err(e);
        }

        // 流式发送
        let network = self.network.clone();
        let progress_tx = self.progress_tx.clone();
        let cancel_tokens = self.cancel_tokens.clone();
        let tid = transfer_id.clone();
        let fname = file_name.clone();
        let tid_for_cleanup = transfer_id.clone();
        let tid_for_cancel = transfer_id.clone();
        let file_path_owned = file_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let mut file = match std::fs::File::open(&file_path_owned) {
                Ok(f) => f,
                Err(e) => {
                    send_failed(&progress_tx, &tid, &fname, file_size,
                        &format!("打开文件失败: {}", e), Some(target_id_owned.clone()));
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }
            };

            let mut buf = vec![0u8; CHUNK_SIZE];

            for i in 0..total_chunks {
                if cancel.load(Ordering::Relaxed) {
                    send_failed(&progress_tx, &tid, &fname, file_size,
                        "传输已取消", Some(target_id_owned.clone()));
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }

                let offset = i as u64 * CHUNK_SIZE as u64;
                let chunk_len = std::cmp::min(CHUNK_SIZE as u64, file_size - offset) as usize;

                if let Err(e) = file.seek(SeekFrom::Start(offset)) {
                    send_failed(&progress_tx, &tid, &fname, file_size,
                        &format!("读取文件失败: {}", e), Some(target_id_owned.clone()));
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }
                let chunk = &mut buf[..chunk_len];
                if let Err(e) = file.read_exact(chunk) {
                    send_failed(&progress_tx, &tid, &fname, file_size,
                        &format!("读取文件失败: {}", e), Some(target_id_owned.clone()));
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }

                let chunk_data = chunk.to_vec();
                let msg = Message::FileDataChunk(FileDataChunkPayload {
                    transfer_id: tid.clone(),
                    chunk_index: i,
                    total_chunks,
                    data: chunk_data,
                    sha256_chunk: String::new(),
                });
                let encoded = match msg.encode() {
                    Ok(e) => e,
                    Err(e) => {
                        send_failed(&progress_tx, &tid, &fname, file_size,
                            &format!("编码失败: {}", e), Some(target_id_owned.clone()));
                        cancel_tokens.remove(&tid_for_cancel);
                        return;
                    }
                };

                if let Err(e) = network.send_raw(&target_id_owned, encoded) {
                    send_failed(&progress_tx, &tid, &fname, file_size,
                        &format!("发送失败: {}", e), Some(target_id_owned.clone()));
                    cancel_tokens.remove(&tid_for_cancel);
                    return;
                }

                let bytes_sent = offset + chunk_len as u64;
                let _ = progress_tx.send(TransferProgress {
                    transfer_id: tid.clone(),
                    file_name: fname.clone(),
                    file_size,
                    bytes_transferred: bytes_sent,
                    progress: bytes_sent as f64 / file_size as f64,
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
            received_count: 0,
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

    /// 取消传输（发送端/用户调用）
    pub fn cancel_transfer(&self, transfer_id: &str) -> AppResult<()> {
        if let Some(token) = self.cancel_tokens.get(transfer_id) {
            token.store(true, Ordering::Relaxed);
        }

        // 清理接收端会话 + 删除未完成的文件
        if let Some(removed) = self.incoming.lock().remove(transfer_id) {
            let partial_path = self.save_dir.join(&removed.file_name);
            let _ = std::fs::remove_file(&partial_path); // 忽略删除失败

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
            let partial_path = self.save_dir.join(&removed.file_name);
            let _ = std::fs::remove_file(&partial_path);

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

    /// 处理文件数据块：流式直接写入磁盘（不阻塞网络读循环）
    pub fn handle_file_chunk(&self, payload: &FileDataChunkPayload) -> AppResult<()> {
        let tid = payload.transfer_id.clone();
        let chunk_index = payload.chunk_index;
        let chunk_data = payload.data.clone();

        // 先读元数据（锁内），写完盘后再更新计数
        let (fname, file_size, src_id, total_chunks) = {
            let incoming = self.incoming.lock();
            let transfer = incoming
                .get(&payload.transfer_id)
                .ok_or_else(|| AppError::Transfer("未知传输会话".into()))?;
            (transfer.file_name.clone(), transfer.file_size, transfer.source_device_id.clone(), transfer.total_chunks)
        };

        // 异步写盘（block_in_place 避免阻塞 tokio 运行时 + 网络读循环）
        let save_dir = self.save_dir.clone();
        let fname_clone = fname.clone();
        let chunk_size = CHUNK_SIZE as u64;
        let result = tokio::task::block_in_place(|| {
            let save_path = save_dir.join(&fname_clone);
            if chunk_index == 0 {
                std::fs::write(&save_path, &chunk_data)
                    .map_err(|e| AppError::Transfer(format!("写入文件失败: {}", e)))
            } else {
                let mut file = std::fs::OpenOptions::new()
                    .write(true).create(true)
                    .open(&save_path)
                    .map_err(|e| AppError::Transfer(format!("打开文件失败: {}", e)))?;
                file.seek(SeekFrom::Start(chunk_index as u64 * chunk_size))
                    .map_err(|e| AppError::Transfer(format!("seek 失败: {}", e)))?;
                file.write_all(&chunk_data)
                    .map_err(|e| AppError::Transfer(format!("写入文件失败: {}", e)))
            }
        });

        if let Err(err_msg) = &result {
            // 写盘失败：清理会话 + 删除部分文件 + 通知前端
            let save_path = self.save_dir.join(&fname);
            self.incoming.lock().remove(&tid);
            let _ = std::fs::remove_file(&save_path);
            let _ = self.progress_tx.send(TransferProgress {
                transfer_id: tid, file_name: fname, file_size,
                bytes_transferred: 0, progress: 0.0,
                state: TransferState::Failed, error: Some(err_msg.to_string()),
                save_path: None, device_id: Some(src_id),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });
            return Ok(());
        }

        // 重新获取锁，更新计数
        let mut incoming = self.incoming.lock();
        let transfer = match incoming.get_mut(&tid) {
            Some(t) => t,
            None => return Ok(()), // 可能已被取消
        };
        transfer.received_count += 1;

        let received = transfer.received_count as u64;
        let total = transfer.total_chunks as u64;
        let progress = if total == 0 { 1.0 } else { received as f64 / total as f64 };

        let src_id = transfer.source_device_id.clone();
        let _ = self.progress_tx.send(TransferProgress {
            transfer_id: payload.transfer_id.clone(),
            file_name: transfer.file_name.clone(),
            file_size: transfer.file_size,
            bytes_transferred: std::cmp::min(received * CHUNK_SIZE as u64, transfer.file_size),
            progress,
            state: TransferState::Transferring,
            error: None,
            save_path: None,
            device_id: Some(src_id),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        });

        if transfer.received_count == transfer.total_chunks {
            let transfer = incoming.remove(&payload.transfer_id).unwrap();
            drop(incoming);

            let save_path = self.save_dir.join(&transfer.file_name);
            let _ = self.progress_tx.send(TransferProgress {
                transfer_id: payload.transfer_id.clone(),
                file_name: transfer.file_name.clone(),
                file_size: transfer.file_size,
                bytes_transferred: transfer.file_size,
                progress: 1.0,
                state: TransferState::Completed,
                error: None,
                save_path: Some(save_path.to_string_lossy().into_owned()),
                device_id: Some(transfer.source_device_id),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });
        }

        Ok(())
    }

    /// 获取默认保存目录
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }
}
