import { useEffect, useRef } from 'react';
import { Monitor, SendHorizonal, RefreshCw } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useDeviceOnline, useDeviceOffline } from '../hooks/useTauriEvents';
import { sendFiles } from '../hooks/useTauriCommands';
import { useDragDropFiles } from '../hooks/useDragDropFiles';
import type { DiscoveredDevice } from '../types';

function deviceInitial(name: string): string {
  return (name.charAt(0) || '?').toUpperCase();
}

function deviceColor(name: string): string {
  const colors = [
    'from-amber-500 to-orange-500',
    'from-blue-500 to-indigo-500',
    'from-emerald-500 to-teal-500',
    'from-violet-500 to-purple-500',
    'from-rose-500 to-pink-500',
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

export function DeviceList() {
  const devices = useAppStore((s) => s.devices);
  const config = useAppStore((s) => s.config);
  const addDevice = useAppStore((s) => s.addDevice);
  const removeDevice = useAppStore((s) => s.removeDevice);
  const loadConfig = useAppStore((s) => s.loadConfig);
  const startupRefresh = useAppStore((s) => s.startupRefresh);
  const loadDevices = useAppStore((s) => s.loadDevices);
  const loaded = useRef(false);

  // 初始加载（仅一次，含延迟刷新捕获慢设备）
  useEffect(() => {
    if (loaded.current) return;
    loaded.current = true;
    loadConfig();
    startupRefresh();
  }, [loadConfig]);

  useDeviceOnline((device) => {
    addDevice(device as DiscoveredDevice);
  });

  useDeviceOffline((deviceId) => {
    removeDevice(deviceId);
  });

  // 拖放文件传输
  const { dragState, hoveredDeviceId } = useDragDropFiles();

  const handleSendFile = async (targetDeviceId: string) => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ multiple: true });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        await sendFiles(paths, targetDeviceId);
      }
    } catch (e) {
      console.error('选择文件失败:', e);
    }
  };

  return (
    <div>
      {/* 标题 */}
      <div className="flex items-center gap-2 mb-4">
        <div className="w-1 h-4 rounded-full bg-amber-500" />
        <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">在线设备</h2>
        <span className="bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400 text-[10px] px-1.5 py-0.5 rounded-full">
          {devices.length}
        </span>
        <button
          onClick={() => { loadDevices(); }}
          className="ml-auto p-1 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
          title="刷新设备列表"
        >
          <RefreshCw className="w-4 h-4 text-slate-400 dark:text-slate-500" />
        </button>
      </div>

      {devices.length === 0 ? (
        <div className="text-center py-14">
          <div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-slate-100 dark:bg-slate-800 flex items-center justify-center">
            <Monitor className="w-7 h-7 text-slate-300 dark:text-slate-600" />
          </div>
          <p className="text-sm text-slate-500 dark:text-slate-400">未发现其他设备</p>
          <p className="text-xs text-slate-400 dark:text-slate-500 mt-1.5">
            请确保两台设备在同一局域网内
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {devices.map((device) => (
            <div
              key={device.device_id}
              data-drop-device={device.device_id}
              className={`flex items-center gap-3 p-3 rounded-xl bg-white dark:bg-slate-800 shadow-sm transition-all duration-200${
                dragState.phase === 'dragging'
                  ? ' border-2 border-dashed border-amber-300/60 dark:border-amber-500/40 cursor-copy'
                  : ' border border-slate-200 dark:border-slate-700 hover:shadow-md hover:border-slate-300 dark:hover:border-slate-600'
              }${
                hoveredDeviceId === device.device_id
                  ? ' !border-amber-400 dark:!border-amber-400 !border-solid scale-[1.02] shadow-lg shadow-amber-500/20 drop-target-glow bg-amber-50/50 dark:bg-amber-500/5'
                  : ''
              }${
                dragState.phase === 'dragging' && hoveredDeviceId !== device.device_id
                  ? ' opacity-50 scale-[0.98]'
                  : ''
              }`}
            >
              {/* 首字母头像 */}
              <div
                className={`w-9 h-9 rounded-xl bg-gradient-to-br ${deviceColor(device.device_name)} flex items-center justify-center flex-shrink-0`}
              >
                <span className="text-white font-semibold text-sm">
                  {deviceInitial(device.device_name)}
                </span>
              </div>

              {/* 设备信息 */}
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-slate-800 dark:text-slate-200 truncate">
                  {device.device_name}
                </p>
                <p className="text-[11px] text-slate-400 dark:text-slate-500">
                  {device.ip_address}
                </p>
              </div>

              {/* 端口标签 + 操作 */}
              <div className="flex items-center gap-2">
                <span className="bg-slate-100 dark:bg-slate-700 text-slate-500 dark:text-slate-400 text-[10px] px-2 py-0.5 rounded-md">
                  :{device.tcp_port}
                </span>
                {/* 发送文件按钮 */}
                <button
                  onClick={() => handleSendFile(device.device_id)}
                  className="p-1.5 rounded-lg hover:bg-amber-50 dark:hover:bg-amber-500/10 transition-colors"
                  title="发送文件"
                >
                  <SendHorizonal className="w-4 h-4 text-slate-400 dark:text-slate-500 hover:text-amber-500" />
                </button>
                <span className="w-2 h-2 rounded-full bg-emerald-500 shadow-[0_0_5px_rgba(16,185,129,0.3)]" />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 本机信息 */}
      <div className="mt-6 p-3 rounded-xl bg-slate-100/70 dark:bg-slate-800/70 border border-slate-200/70 dark:border-slate-700/70">
        <div className="flex items-center justify-between mb-1">
          <span className="text-[10px] text-slate-400 dark:text-slate-500 uppercase tracking-wider">本机</span>
          <span className="bg-emerald-50 dark:bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-500/30 text-[10px] px-2 py-0.5 rounded-full">
            在线
          </span>
        </div>
        <p className="text-sm font-medium text-slate-700 dark:text-slate-300">
          {config?.device_name ?? '此设备'}
        </p>
        <p className="text-[11px] text-slate-400 dark:text-slate-500 mt-1">
          端口 {config?.tcp_port ?? 54322} · 等待其他设备连接...
        </p>
      </div>
    </div>
  );
}
