import { invoke } from '@tauri-apps/api/core';
import type { AppConfig, SyncStats, ClipboardEntry } from '../types';

// ── 获取在线设备列表 ────────────────────
export async function getDevices(): Promise<unknown[]> {
  return invoke('get_devices');
}

// ── 切换同步开关 ───────────────────────
export async function toggleSync(): Promise<boolean> {
  return invoke('toggle_sync');
}

// ── 获取同步状态 ───────────────────────
export async function isSyncEnabled(): Promise<boolean> {
  return invoke('is_sync_enabled');
}

// ── 更新设备名称 ───────────────────────
export async function updateDeviceName(name: string): Promise<void> {
  return invoke('update_device_name', { name });
}

// ── 获取配置 ──────────────────────────
export async function getConfig(): Promise<AppConfig> {
  return invoke('get_config');
}

// ── 更新整个配置 ───────────────────────
export async function updateConfig(config: AppConfig): Promise<void> {
  return invoke('update_config', { newConfig: config });
}

// ── 获取同步统计 ───────────────────────
export async function getSyncStats(): Promise<SyncStats> {
  return invoke('get_sync_stats');
}

// ── 发送文件 ──────────────────────────
export async function sendFiles(paths: string[], target: string): Promise<void> {
  return invoke('send_files', { paths, target });
}

// ── 获取剪贴板历史 ──────────────────────
export async function getClipboardHistory(): Promise<ClipboardEntry[]> {
  return invoke('get_clipboard_history');
}

// ── 从历史重新复制 ──────────────────────
export async function copyFromHistory(entryId: string): Promise<void> {
  return invoke('copy_from_history', { entryId });
}
