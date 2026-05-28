import { useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── 设备上线事件 ────────────────────────
export function useDeviceOnline(callback: (device: unknown) => void) {
  const cbRef = useRef(callback);
  cbRef.current = callback;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('device-online', (event) => {
      cbRef.current(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []); // 只注册一次，通过 ref 保持回调最新
}

// ── 设备下线事件 ────────────────────────
export function useDeviceOffline(callback: (deviceId: string) => void) {
  const cbRef = useRef(callback);
  cbRef.current = callback;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('device-offline', (event) => {
      cbRef.current(event.payload as string);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []); // 只注册一次
}

// ── 传输进度事件 ────────────────────────
export function useTransferProgress(
  callback: (progress: { file_name: string; progress: number; state: string }) => void
) {
  const cbRef = useRef(callback);
  cbRef.current = callback;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('transfer-progress', (event) => {
      cbRef.current(event.payload as { file_name: string; progress: number; state: string });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []); // 只注册一次
}

// ── 剪贴板更新事件（远端同步到本地时触发）───
export function useClipboardUpdated(callback: (entry: unknown) => void) {
  const cbRef = useRef(callback);
  cbRef.current = callback;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    listen('clipboard-updated', (event) => {
      cbRef.current(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []); // 只注册一次
}
