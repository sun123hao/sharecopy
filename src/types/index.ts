// ── 设备信息 ──────────────────────────
export interface DiscoveredDevice {
  device_id: string;
  device_name: string;
  hostname: string;
  platform: string;
  ip_address: string;
  tcp_port: number;
}

// ── 应用配置 ──────────────────────────
export interface AppConfig {
  device_name: string;
  device_id: string;
  tcp_port: number;
  save_dir: string;
  auto_start: boolean;
  sync_enabled: boolean;
  auto_accept_files: boolean;
  poll_interval_active_ms: number;
  poll_interval_idle_ms: number;
  history_retention_days: number; // 0=关闭即清, 1/3/5=保留天数
  transfer_retention_days: number; // 0=关闭即清, 1/3/5=保留天数
}

// ── 同步统计 ──────────────────────────
export interface SyncStats {
  texts_synced: number;
  images_synced: number;
  files_transferred: number;
}

// ── 传输进度 ──────────────────────────
export type TransferState = 'pending' | 'transferring' | 'completed' | 'failed';

export interface TransferProgress {
  transfer_id: string;
  file_name: string;
  progress: number; // 0-100
  state: TransferState;
  device_id?: string; // 关联的设备 ID（发送端=目标设备，接收端=来源设备）
  timestamp?: number; // 传输完成/失败时间戳（毫秒）
  save_path?: string; // 接收端保存路径
}

// ── 剪贴板历史条目 ────────────────────
export interface ClipboardEntry {
  id: string;
  type: 'text' | 'image';
  content: string; // 文本内容或图片 base64
  from_device: string;
  timestamp: number;
}
