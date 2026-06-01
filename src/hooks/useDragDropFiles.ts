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
  const [hoveredDeviceId, setHoveredDevice] = useState<string | null>(null);

  /** 根据平台获取标题栏高度（Tauri 窗口坐标 → webview 内容坐标的 Y 轴偏移） */
  const titleBarHeight = useRef(0);

  /** 设备卡片 bounding rect 缓存，避免 over 事件中反复触发强制同步布局 */
  const cardsRectsRef = useRef<Map<string, DOMRect>>(new Map());

  /** 是否需要对坐标做 DPR 缩放（Windows/Linux 返回物理像素，macOS 已返回逻辑像素） */
  const needsDprScale = useRef(false);

  /** 将 Tauri 窗口坐标转为 webview 内容区 CSS 坐标 */
  const toCssPosition = useCallback((x: number, y: number) => {
    const dpr = needsDprScale.current ? (window.devicePixelRatio || 1) : 1;
    return { x: x / dpr, y: y / dpr - titleBarHeight.current };
  }, []);

  /** 通过坐标命中检测找到目标设备 ID（精确匹配：光标正在卡片元素上） */
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

  /**
   * 就近匹配回退：当光标不在具体卡片上时（如卡片间隙、边距区域），
   * 按 Y 坐标找到最近的设备卡片。使用缓存的 bounding rect 避免强制布局。
   */
  const findNearestDevice = useCallback((x: number, y: number): string | null => {
    const cached = cardsRectsRef.current;
    if (cached.size === 0) return null;

    let closest: string | null = null;
    let minDist = Infinity;
    cached.forEach((rect, deviceId) => {
      // X 轴落在卡片水平范围内（含少量容差，确保宽度足够）
      if (x < rect.left - 10 || x > rect.right + 10) return;
      // 计算到卡片的垂直距离（光标在卡片内部时距离为 0）
      const dist = y < rect.top ? rect.top - y
        : y > rect.bottom ? y - rect.bottom
        : 0;
      if (dist < minDist) {
        minDist = dist;
        closest = deviceId;
      }
    });
    return closest;
  }, []);

  /** 更新悬停设备（仅在值变化时触发 state 更新，避免 over 事件频繁重渲染） */
  const updateHoveredDevice = useCallback((deviceId: string | null) => {
    const prev = hoveredDeviceIdRef.current;
    hoveredDeviceIdRef.current = deviceId;
    if (prev !== deviceId) {
      setHoveredDevice(deviceId);
    }
  }, []);

  /** 缓存所有设备卡片的 bounding rect（在 enter 时调用，避免 over 事件中反复查询） */
  const cacheCards = useCallback(() => {
    const map = cardsRectsRef.current;
    map.clear();
    document.querySelectorAll('[data-drop-device]').forEach((card) => {
      const id = (card as HTMLElement).dataset.dropDevice;
      if (id) map.set(id, card.getBoundingClientRect());
    });
  }, []);

  /** 重置所有拖放状态 */
  const resetDrag = useCallback(() => {
    setDragState({ phase: 'idle', position: null, fileCount: 0 });
    hoveredDeviceIdRef.current = null;
    setHoveredDevice(null);
    cardsRectsRef.current.clear();
    document.body.classList.remove('is-dragging');
  }, []);

  useEffect(() => {
    // 检测平台设置标题栏高度和坐标缩放策略
    const platform = navigator.platform || '';
    if (platform.includes('Win')) {
      titleBarHeight.current = 32; // Windows 标题栏
      needsDprScale.current = true; // Windows 返回物理像素，需 DPR 缩放
    } else if (platform.includes('Mac')) {
      titleBarHeight.current = 0; // macOS 坐标相对于内容区，无需偏移
      needsDprScale.current = false; // macOS 已返回逻辑像素，无需缩放
    } else if (platform.includes('Linux')) {
      titleBarHeight.current = 30; // Linux (GNOME/KDE) 标题栏
      needsDprScale.current = true; // Linux 返回物理像素，需 DPR 缩放
    } else {
      titleBarHeight.current = 0;
    }

    let cancelled = false;
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
            // 缓存设备卡片位置，避免 over 事件中反复触发强制布局
            cacheCards();
            document.body.classList.add('is-dragging');
            break;
          }

          case 'over': {
            const pos = toCssPosition(
              event.payload.position.x,
              event.payload.position.y,
            );
            setDragState(prev => ({ ...prev, position: pos }));
            // 精确命中优先，回退到就近匹配（解决卡片间隙无法匹配的问题）
            const deviceId = findDeviceAtPoint(pos.x, pos.y) ?? findNearestDevice(pos.x, pos.y);
            updateHoveredDevice(deviceId);
            break;
          }

          case 'drop': {
            const paths = event.payload.paths;
            const pos = toCssPosition(
              event.payload.position.x,
              event.payload.position.y,
            );

            // 优先级：over 记录 > 精确命中 > 就近匹配
            const lastHovered = hoveredDeviceIdRef.current;
            const targetDeviceId = lastHovered
              ?? findDeviceAtPoint(pos.x, pos.y)
              ?? findNearestDevice(pos.x, pos.y);

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
      if (cancelled) {
        // 组件已在 await 期间卸载，立即清理刚注册的监听器
        cleanup();
      } else {
        unlisten = cleanup;
      }
    };

    setup();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
      document.body.classList.remove('is-dragging');
    };
  }, [toCssPosition, findDeviceAtPoint, findNearestDevice, cacheCards, updateHoveredDevice, resetDrag]);

  return { dragState, hoveredDeviceId };
}
