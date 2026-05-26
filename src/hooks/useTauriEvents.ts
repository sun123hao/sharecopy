import { useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── 设备上线事件 ────────────────────────
export function useDeviceOnline(callback: (device: unknown) => void) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('device-online', (event) => {
      callback(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [callback]);
}

// ── 设备下线事件 ────────────────────────
export function useDeviceOffline(callback: (deviceId: string) => void) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('device-offline', (event) => {
      callback(event.payload as string);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [callback]);
}

// ── 传输进度事件 ────────────────────────
export function useTransferProgress(
  callback: (progress: { file_name: string; progress: number; state: string }) => void
) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('transfer-progress', (event) => {
      callback(event.payload as { file_name: string; progress: number; state: string });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [callback]);
}

// ── 剪贴板更新事件（远端同步到本地时触发）───
export function useClipboardUpdated(callback: (entry: unknown) => void) {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('clipboard-updated', (event) => {
      callback(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [callback]);
}
