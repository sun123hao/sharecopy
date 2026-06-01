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

/* iOS 18+ 系统色头像渐变 */
function deviceColor(name: string): string {
  const colors = [
    'from-accent to-[#5856D6]',
    'from-ios-green to-[#30B44A]',
    'from-ios-orange to-[#FF7A00]',
    'from-ios-purple to-[#8B3FB5]',
    'from-ios-pink to-[#D91F3F]',
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

export function DeviceList({ onSelectDevice }: { onSelectDevice?: (deviceId: string) => void }) {
  const devices = useAppStore((s) => s.devices);
  const addDevice = useAppStore((s) => s.addDevice);
  const removeDevice = useAppStore((s) => s.removeDevice);
  const loadConfig = useAppStore((s) => s.loadConfig);
  const startupRefresh = useAppStore((s) => s.startupRefresh);
  const loadDevices = useAppStore((s) => s.loadDevices);
  const loaded = useRef(false);

  useEffect(() => {
    if (loaded.current) return;
    loaded.current = true;
    loadConfig();
    startupRefresh();
  }, [loadConfig]);

  useDeviceOnline((device) => { addDevice(device as DiscoveredDevice); });
  useDeviceOffline((deviceId) => { removeDevice(deviceId); });

  const { dragState, hoveredDeviceId } = useDragDropFiles();

  const handleSendFile = async (targetDeviceId: string) => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ multiple: true });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        await sendFiles(paths, targetDeviceId);
      }
    } catch (e) { console.error('选择文件失败:', e); }
  };

  return (
    <div>
      {/* 标题 */}
      <div className="flex items-center gap-2 mb-3 px-1">
        <span className="text-[13px] font-medium text-black/40 dark:text-white/40 uppercase tracking-[0.02em]">
          在线设备
        </span>
        <span className="ios-card border border-ios-separator dark:border-ios-separator-dark text-[11px] font-medium text-black/40 dark:text-white/40 px-2 py-0.5 rounded-full">
          {devices.length}
        </span>
        <button
          onClick={() => { loadDevices(); }}
          className="ml-auto p-1.5 rounded-[10px] hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
          title="刷新"
        >
          <RefreshCw className="w-[18px] h-[18px] text-black/20 dark:text-white/20" />
        </button>
      </div>

      {devices.length === 0 ? (
        <div className="text-center py-16">
          <div className="w-[68px] h-[68px] mx-auto mb-4 rounded-[20px] ios-card border border-white/10 flex items-center justify-center">
            <Monitor className="w-7 h-7 text-black/15 dark:text-white/15" />
          </div>
          <p className="text-[15px] text-black/60 dark:text-white/60">未发现其他设备</p>
          <p className="text-[13px] text-black/30 dark:text-white/30 mt-1.5">
            请确保两台设备在同一局域网内
          </p>
        </div>
      ) : (
        <div className="space-y-[10px]">
          {devices.map((device) => (
            <div
              key={device.device_id}
              data-drop-device={device.device_id}
              role="button"
              tabIndex={0}
              onClick={() => {
                if (dragState.phase === 'dragging') return;
                onSelectDevice?.(device.device_id);
              }}
              className={`w-full flex items-center gap-3 p-[15px] rounded-[20px] ios-card border border-white/15 dark:border-white/5 transition-all duration-300 cursor-pointer
                active:scale-[0.985] active:bg-ios-card active:shadow-[0_2px_8px_rgba(0,0,0,0.06)]
                ${hoveredDeviceId === device.device_id
                  ? '!border-accent/40 !bg-accent/[0.06] scale-[1.01] drop-target-glow'
                  : dragState.phase === 'dragging' && hoveredDeviceId !== device.device_id
                    ? 'opacity-40 scale-[0.98]'
                    : ''
                }
                ${dragState.phase === 'dragging'
                  ? 'border-dashed border-accent/30'
                  : ''
                }`}
            >
              {/* 头像 */}
              <div
                className={`w-[42px] h-[42px] rounded-[12px] bg-gradient-to-br ${deviceColor(device.device_name)} flex items-center justify-center flex-shrink-0 shadow-[inset_0_1px_0_rgba(255,255,255,0.2)]`}
              >
                <span className="text-white font-semibold text-[17px]">
                  {deviceInitial(device.device_name)}
                </span>
              </div>

              <div className="flex-1 min-w-0">
                <p className="text-[15px] font-medium truncate">{device.device_name}</p>
                <p className="text-[12px] text-black/40 dark:text-white/40">
                  {device.ip_address}
                </p>
              </div>

              <div className="flex items-center gap-[10px]">
                <span className="ios-card border border-ios-separator dark:border-white/5 text-[11px] font-medium text-black/40 dark:text-white/40 px-2 py-0.5 rounded-[7px] min-w-[52px] inline-block text-center">
                  :{device.tcp_port}
                </span>
                <button
                  onClick={(e) => { e.stopPropagation(); handleSendFile(device.device_id); }}
                  className="p-1.5 rounded-[9px] hover:bg-black/5 dark:hover:bg-white/5 text-black/20 dark:text-white/20 hover:text-accent transition-all duration-200"
                  title="发送文件"
                >
                  <SendHorizonal className="w-[17px] h-[17px]" />
                </button>
                <span className="w-[9px] h-[9px] rounded-full bg-ios-green shadow-[0_0_8px_rgba(52,199,89,0.5)]" />
              </div>
            </div>
          ))}
        </div>
      )}

    </div>
  );
}
