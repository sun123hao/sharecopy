import { useEffect } from 'react';
import { Monitor, SendHorizonal } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useDeviceOnline, useDeviceOffline } from '../hooks/useTauriEvents';
import { sendFiles } from '../hooks/useTauriCommands';
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

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  useDeviceOnline((device) => {
    addDevice(device as DiscoveredDevice);
  });

  useDeviceOffline((deviceId) => {
    removeDevice(deviceId);
  });

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
        <h2 className="text-sm font-semibold text-slate-700">在线设备</h2>
        <span className="bg-slate-200 text-slate-500 text-[10px] px-1.5 py-0.5 rounded-full">
          {devices.length}
        </span>
      </div>

      {devices.length === 0 ? (
        <div className="text-center py-14">
          <div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-slate-100 flex items-center justify-center">
            <Monitor className="w-7 h-7 text-slate-300" />
          </div>
          <p className="text-sm text-slate-500">未发现其他设备</p>
          <p className="text-xs text-slate-400 mt-1.5">
            请确保两台设备在同一局域网内
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {devices.map((device) => (
            <div
              key={device.device_id}
              className="flex items-center gap-3 p-3 rounded-xl bg-white border border-slate-200 shadow-sm hover:shadow-md hover:border-slate-300 transition-all"
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
                <p className="text-sm font-medium text-slate-800 truncate">
                  {device.device_name}
                </p>
                <p className="text-[11px] text-slate-400">
                  {device.ip_address}
                </p>
              </div>

              {/* 端口标签 + 操作 */}
              <div className="flex items-center gap-2">
                <span className="bg-slate-100 text-slate-500 text-[10px] px-2 py-0.5 rounded-md">
                  :{device.tcp_port}
                </span>
                <button
                  onClick={() => handleSendFile(device.device_id)}
                  className="p-1.5 rounded-lg hover:bg-amber-50 transition-colors"
                  title="发送文件"
                >
                  <SendHorizonal className="w-4 h-4 text-slate-400 hover:text-amber-500" />
                </button>
                <span className="w-2 h-2 rounded-full bg-emerald-500 shadow-[0_0_5px_rgba(16,185,129,0.3)]" />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 本机信息 */}
      <div className="mt-6 p-3 rounded-xl bg-slate-100/70 border border-slate-200/70">
        <div className="flex items-center justify-between mb-1">
          <span className="text-[10px] text-slate-400 uppercase tracking-wider">本机</span>
          <span className="bg-emerald-50 text-emerald-600 border border-emerald-200 text-[10px] px-2 py-0.5 rounded-full">
            在线
          </span>
        </div>
        <p className="text-sm font-medium text-slate-700">
          {config?.device_name ?? '此设备'}
        </p>
        <p className="text-[11px] text-slate-400 mt-1">
          端口 {config?.tcp_port ?? 54322} · 等待其他设备连接...
        </p>
      </div>
    </div>
  );
}
