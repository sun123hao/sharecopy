import { ArrowLeft, SendHorizonal } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { sendFiles } from '../hooks/useTauriCommands';
import { TransferProgressPanel } from './TransferProgress';

function deviceInitial(name: string): string {
  return (name.charAt(0) || '?').toUpperCase();
}

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

export function DeviceTransferPage({ onBack }: { onBack: () => void }) {
  const selectedDeviceId = useAppStore((s) => s.selectedDeviceId);
  const devices = useAppStore((s) => s.devices);
  const device = devices.find((d) => d.device_id === selectedDeviceId);

  const handleSendFile = async () => {
    if (!device) return;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ multiple: true });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        await sendFiles(paths, device.device_id);
      }
    } catch (e) { console.error('选择文件失败:', e); }
  };

  if (!device) {
    return (
      <div className="flex flex-col items-center py-16">
        <p className="text-[15px] text-black/60 dark:text-white/60 mb-4">设备已离线</p>
        <button
          onClick={onBack}
          className="flex items-center gap-1.5 px-5 py-2.5 rounded-[10px] glass-chrome border border-white/10 text-accent text-[15px] font-medium hover:scale-[0.98] transition-all duration-300"
        >
          <ArrowLeft className="w-[18px] h-[18px]" />
          返回设备列表
        </button>
      </div>
    );
  }

  return (
    <div>
      {/* 设备信息头部 */}
      <div className="flex items-center gap-3 mb-5">
        <button
          onClick={onBack}
          className="w-[34px] h-[34px] rounded-[10px] glass-chrome border border-white/10 flex items-center justify-center text-accent hover:scale-95 active:scale-90 transition-all duration-200 flex-shrink-0"
        >
          <ArrowLeft className="w-[20px] h-[20px]" />
        </button>

        <div
          className={`w-[44px] h-[44px] rounded-[13px] bg-gradient-to-br ${deviceColor(device.device_name)} flex items-center justify-center flex-shrink-0 shadow-[inset_0_1px_0_rgba(255,255,255,0.2)]`}
        >
          <span className="text-white font-semibold text-[18px]">
            {deviceInitial(device.device_name)}
          </span>
        </div>

        <div className="flex-1 min-w-0">
          <p className="text-[17px] font-semibold truncate">{device.device_name}</p>
          <p className="text-[13px] text-black/40 dark:text-white/40">
            {device.ip_address} · 端口 {device.tcp_port}
          </p>
        </div>

        <button
          onClick={handleSendFile}
          className="w-[38px] h-[38px] rounded-[11px] bg-accent text-white flex items-center justify-center shadow-[0_2px_8px_rgba(0,122,255,0.35)] hover:scale-95 active:scale-90 transition-all duration-200 flex-shrink-0"
        >
          <SendHorizonal className="w-[18px] h-[18px]" />
        </button>
      </div>

      {/* 传输记录 */}
      <TransferProgressPanel deviceId={selectedDeviceId ?? undefined} />
    </div>
  );
}
