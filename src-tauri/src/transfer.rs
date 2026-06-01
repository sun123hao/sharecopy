use std::collections::{HashMap, HashSet};
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
    file_name: String,       // 目标文件名
    temp_path: PathBuf,      // 传输中临时文件路径（避免同名冲突）
    file_size: u64,
    total_chunks: u32,
    received: HashSet<u32>,  // 已收到的 chunk 索引（去重）
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
    incoming: Arc<parking_lot::Mutex<HashMap<String, IncomingTransfer>>>,
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
            incoming: Arc::new(parking_lot::Mutex::new(HashMap::new())),
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
        // 临时文件名：{name}.{transfer_id前8位}.tmp（避免同名冲突）
        let short_id = &payload.transfer_id[..payload.transfer_id.len().min(8)];
        let temp_name = format!("{}.{}.tmp", payload.file_name, short_id);
        let temp_path = self.save_dir.join(&temp_name);

        // 预分配文件大小（减少 Android FUSE 反复扩展文件的开销）
        if payload.file_size > 0 {
            match std::fs::File::create(&temp_path) {
                Ok(file) => {
                    if let Err(e) = file.set_len(payload.file_size) {
                        tracing::warn!("预分配文件大小失败 ({}): {}", temp_path.display(), e);
                    }
                }
                Err(e) => {
                    tracing::warn!("创建临时文件失败 ({}): {}", temp_path.display(), e);
                }
            }
        }

        let transfer = IncomingTransfer {
            _transfer_id: payload.transfer_id.clone(),
            source_device_id: source_device_id.to_string(),
            file_name: payload.file_name.clone(),
            temp_path,
            file_size: payload.file_size,
            total_chunks: payload.total_chunks,
            received: HashSet::new(),
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

        // 清理接收端会话 + 删除传输中的临时文件
        if let Some(removed) = self.incoming.lock().remove(transfer_id) {
            let _ = std::fs::remove_file(&removed.temp_path);
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
            let _ = std::fs::remove_file(&removed.temp_path);
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

    /// 处理文件数据块：后台写盘，不等待磁盘，立即返回继续读网络
    pub fn handle_file_chunk(&self, payload: &FileDataChunkPayload) -> AppResult<()> {
        let tid = payload.transfer_id.clone();
        let chunk_index = payload.chunk_index;
        let chunk_data = payload.data.clone();
        let total_chunks = payload.total_chunks;

        // 读元数据 + 去重检查
        let (fname, file_size, src_id, temp_path) = {
            let mut incoming = self.incoming.lock();
            let transfer = incoming
                .get_mut(&tid)
                .ok_or_else(|| AppError::Transfer("未知传输会话".into()))?;
            // 去重：已收到的 chunk 跳过
            if !transfer.received.insert(chunk_index) {
                return Ok(());
            }
            (transfer.file_name.clone(), transfer.file_size, transfer.source_device_id.clone(), transfer.temp_path.clone())
        };

        // 后台写盘——不阻塞网络读取循环
        let progress_tx = self.progress_tx.clone();
        let incoming = self.incoming.clone();
        let chunk_size = CHUNK_SIZE as u64;
        let tid2 = tid.clone();
        let fname2 = fname.clone();
        let save_dir = self.save_dir.clone();

        tokio::task::spawn_blocking(move || {
            // 快速检查传输是否仍活跃（避免取消后仍写盘）
            {
                let lock = incoming.lock();
                if !lock.contains_key(&tid2) {
                    return; // 已被取消，跳过 I/O
                }
            }

            // 写盘（文件已由 handle_file_request 预创建）
            let write_result = (|| {
                let mut file = std::fs::OpenOptions::new()
                    .write(true) // 不用 create(true)，取消后文件已删除则 open 失败
                    .open(&temp_path)
                    .map_err(|e| AppError::Transfer(format!("打开文件失败: {}", e)))?;
                file.seek(SeekFrom::Start(chunk_index as u64 * chunk_size))
                    .map_err(|e| AppError::Transfer(format!("seek 失败: {}", e)))?;
                file.write_all(&chunk_data)
                    .map_err(|e| AppError::Transfer(format!("写入文件失败: {}", e)))
            })();

            let mut incoming = incoming.lock();

            if let Err(err_msg) = write_result {
                incoming.remove(&tid2);
                let _ = std::fs::remove_file(&temp_path);
                let _ = progress_tx.send(TransferProgress {
                    transfer_id: tid2, file_name: fname2, file_size,
                    bytes_transferred: 0, progress: 0.0,
                    state: TransferState::Failed, error: Some(err_msg.to_string()),
                    save_path: None, device_id: Some(src_id),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
                return;
            }

            let transfer = match incoming.get(&tid2) {
                Some(t) => t,
                None => return, // 已被取消
            };
            let received = transfer.received.len() as u32;
            let progress = if total_chunks == 0 { 1.0 } else { received as f64 / total_chunks as f64 };

            let _ = progress_tx.send(TransferProgress {
                transfer_id: tid2.clone(),
                file_name: transfer.file_name.clone(),
                file_size,
                bytes_transferred: std::cmp::min(received as u64 * chunk_size, file_size),
                progress,
                state: TransferState::Transferring,
                error: None, save_path: None,
                device_id: Some(transfer.source_device_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
            });

            if received == total_chunks {
                // 所有块收齐：重命名临时文件 → 目标文件
                let final_path = save_dir.join(&transfer.file_name);
                if let Err(e) = std::fs::rename(&transfer.temp_path, &final_path) {
                    let _ = progress_tx.send(TransferProgress {
                        transfer_id: tid2,
                        file_name: transfer.file_name.clone(), file_size,
                        bytes_transferred: file_size, progress: 1.0,
                        state: TransferState::Failed,
                        error: Some(format!("重命名文件失败: {}", e)),
                        save_path: None,
                        device_id: Some(transfer.source_device_id.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    });
                    return;
                }
                // 安全移除：仅当 rename 成功后才从 map 删除
                let transfer = incoming.remove(&tid2).unwrap_or_else(|| {
                    panic!("just checked: transfer must exist");
                });
                drop(incoming);

                let _ = progress_tx.send(TransferProgress {
                    transfer_id: tid2,
                    file_name: transfer.file_name.clone(),
                    file_size,
                    bytes_transferred: file_size,
                    progress: 1.0,
                    state: TransferState::Completed,
                    error: None,
                    save_path: Some(final_path.to_string_lossy().into_owned()),
                    device_id: Some(transfer.source_device_id),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                });
            }
        });

        Ok(()) // 立即返回，不等磁盘
    }

    /// 获取默认保存目录
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }
}
