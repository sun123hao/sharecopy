import { useEffect } from 'react';
import { Laptop, Monitor, SendHorizonal } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useDeviceOnline, useDeviceOffline } from '../hooks/useTauriEvents';
import { sendFiles } from '../hooks/useTauriCommands';
import type { DiscoveredDevice } from '../types';

export function DeviceList() {
  const devices = useAppStore((s) => s.devices);
  const config = useAppStore((s) => s.config);
  const addDevice = useAppStore((s) => s.addDevice);
  const removeDevice = useAppStore((s) => s.removeDevice);
  const loadConfig = useAppStore((s) => s.loadConfig);

  // 初始化加载配置
  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  // 监听设备上线/下线事件
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
      <h2 className="text-sm font-medium text-slate-400 mb-4">在线设备</h2>
      {devices.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-slate-500">未发现其他设备</p>
          <p className="text-xs text-slate-600 mt-2">
            请确保两台设备在同一局域网内
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {devices.map((device) => (
            <div
              key={device.device_id}
              className="flex items-center gap-3 p-3 rounded-lg bg-slate-900 border border-slate-800"
            >
              {device.platform === 'macos' ? (
                <Monitor className="w-5 h-5 text-slate-400" />
              ) : (
                <Laptop className="w-5 h-5 text-slate-400" />
              )}
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-white truncate">
                  {device.device_name}
                </p>
                <p className="text-xs text-slate-500">
                  {device.ip_address}:{device.tcp_port}
                </p>
              </div>
              <div className="flex items-center gap-3">
                <button
                  onClick={() => handleSendFile(device.device_id)}
                  className="p-1.5 rounded-md hover:bg-slate-700 transition-colors"
                  title="发送文件"
                >
                  <SendHorizonal className="w-4 h-4 text-slate-400 hover:text-blue-400" />
                </button>
                <span className="w-2 h-2 rounded-full bg-green-500" />
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 本机信息 */}
      <div className="mt-6 p-3 rounded-lg bg-slate-900/50 border border-slate-800/50">
        <p className="text-xs text-slate-500">本机</p>
        <p className="text-sm text-white mt-1">
          {config?.device_name ?? '此设备'} ({navigator.platform})
        </p>
        <p className="text-xs text-slate-600 mt-1">
          端口 {config?.tcp_port ?? 54322} · 等待其他设备连接...
        </p>
      </div>
    </div>
  );
}
