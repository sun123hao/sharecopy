import { Laptop, Monitor } from 'lucide-react';

interface Device {
  device_id: string;
  device_name: string;
  platform: string;
  ip_address: string;
  online: boolean;
}

// 模拟数据 — 后续阶段会从 Rust 后端获取
const mockDevices: Device[] = [
  {
    device_id: '1',
    device_name: 'My MacBook Pro',
    platform: 'macos',
    ip_address: '192.168.1.100',
    online: true,
  },
];

export function DeviceList() {
  return (
    <div>
      <h2 className="text-sm font-medium text-slate-400 mb-4">在线设备</h2>
      {mockDevices.length === 0 ? (
        <div className="text-center py-12">
          <p className="text-slate-500">未发现其他设备</p>
          <p className="text-xs text-slate-600 mt-2">
            请确保两台设备在同一局域网内
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {mockDevices.map((device) => (
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
                <p className="text-xs text-slate-500">{device.ip_address}</p>
              </div>
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-green-500" />
                <span className="text-xs text-green-400">在线</span>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 本机信息 */}
      <div className="mt-6 p-3 rounded-lg bg-slate-900/50 border border-slate-800/50">
        <p className="text-xs text-slate-500">本机</p>
        <p className="text-sm text-white mt-1">此设备 (macOS)</p>
        <p className="text-xs text-slate-600 mt-1">
          剪贴板同步已启用 · 等待其他设备连接...
        </p>
      </div>
    </div>
  );
}
