import { useState, useEffect, useRef, useCallback } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { sendFiles } from './useTauriCommands';

/** 拖放会话状态 */
export interface DragDropState {
  phase: 'idle' | 'dragging';
  position: { x: number; y: number } | null;
  fileCount: number;
}

/**
 * 拖放文件传输 hook
 *
 * 监听 Tauri 原生拖放事件，当用户从桌面/文件管理器拖动文件到
 * 设备卡片上释放时，自动调用 sendFiles 发送给该设备。
 *
 * 使用方式：
 * 1. 调用 hook 获取 dragState 和 hoveredDeviceId
 * 2. 在设备卡片上添加 data-drop-device={device_id} 属性
 * 3. 根据 dragState.phase 和 hoveredDeviceId 调整 UI 样式
 */
export function useDragDropFiles() {
  const [dragState, setDragState] = useState<DragDropState>({
    phase: 'idle',
    position: null,
    fileCount: 0,
  });

  // 当前悬停的设备 ID — 用 ref 避免 over 事件频繁触发 state 更新
  const hoveredDeviceIdRef = useRef<string | null>(null);
  // 用于驱动 UI 重渲染的 state 副本（仅在值变化时更新）
  const [hoveredDeviceId, setHoveredDeviceId] = useState<string | null>(null);

  /** 根据平台获取标题栏高度（Tauri 窗口坐标 → webview 内容坐标的 Y 轴偏移） */
  const titleBarHeight = useRef(0);

  /** 将 Tauri 窗口坐标转为 webview 内容区 CSS 坐标 */
  const toCssPosition = useCallback((x: number, y: number) => {
    return { x, y: y - titleBarHeight.current };
  }, []);

  /** 通过坐标命中检测找到目标设备 ID */
  const findDeviceAtPoint = useCallback((x: number, y: number): string | null => {
    // 使用 elementsFromPoint（复数）处理嵌套元素，比 elementFromPoint 更可靠
    const els = document.elementsFromPoint(x, y);
    for (const el of els) {
      const deviceEl = (el as HTMLElement).closest?.('[data-drop-device]');
      if (deviceEl) {
        return (deviceEl as HTMLElement).dataset.dropDevice ?? null;
      }
    }
    return null;
  }, []);

  /** 更新悬停设备（仅在值变化时触发 state 更新） */
  const updateHoveredDevice = useCallback((deviceId: string | null) => {
    const prev = hoveredDeviceIdRef.current;
    hoveredDeviceIdRef.current = deviceId;
    if (prev !== deviceId) {
      setHoveredDeviceId(deviceId);
    }
  }, []);

  /** 重置所有拖放状态 */
  const resetDrag = useCallback(() => {
    setDragState({ phase: 'idle', position: null, fileCount: 0 });
    hoveredDeviceIdRef.current = null;
    setHoveredDeviceId(null);
    document.body.classList.remove('is-dragging');
  }, []);

  useEffect(() => {
    // 检测平台设置标题栏高度（Tauri 窗口坐标偏移量）
    const platform = navigator.platform || '';
    if (platform.includes('Win')) {
      titleBarHeight.current = 32; // Windows 标题栏
    } else if (platform.includes('Mac')) {
      titleBarHeight.current = 28; // macOS 标题栏
    } else {
      titleBarHeight.current = 0;  // Linux / 无标题栏
    }

    let unlisten: (() => void) | undefined;

    const setup = async () => {
      const webview = getCurrentWebview();
      const cleanup = await webview.onDragDropEvent((event) => {
        switch (event.payload.type) {
          case 'enter': {
            const pos = toCssPosition(
              event.payload.position.x,
              event.payload.position.y,
            );
            setDragState({
              phase: 'dragging',
              position: pos,
              fileCount: event.payload.paths.length,
            });
            document.body.classList.add('is-dragging');
            break;
          }

          case 'over': {
            const pos = toCssPosition(
              event.payload.position.x,
              event.payload.position.y,
            );
            setDragState(prev => ({ ...prev, position: pos }));
            // 命中检测：找到光标下的设备卡片
            const deviceId = findDeviceAtPoint(pos.x, pos.y);
            updateHoveredDevice(deviceId);
            break;
          }

          case 'drop': {
            const paths = event.payload.paths;
            const pos = toCssPosition(
              event.payload.position.x,
              event.payload.position.y,
            );

            // 优先使用 over 事件中记录的最后悬停设备
            const lastHovered = hoveredDeviceIdRef.current;
            const targetDeviceId = lastHovered ?? findDeviceAtPoint(pos.x, pos.y);

            if (targetDeviceId && paths.length > 0) {
              sendFiles(paths, targetDeviceId).catch((err) => {
                console.error('[拖放] 发送文件失败:', err);
              });
            }

            resetDrag();
            break;
          }

          case 'leave': {
            resetDrag();
            break;
          }
        }
      });
      unlisten = cleanup;
    };

    setup();

    return () => {
      // 清理事件监听
      if (unlisten) unlisten();
      // 确保移除拖放状态类
      document.body.classList.remove('is-dragging');
    };
  }, [toCssPosition, findDeviceAtPoint, updateHoveredDevice, resetDrag]);

  return { dragState, hoveredDeviceId };
}
