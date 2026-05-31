import { ArrowLeft, SendHorizonal } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { sendFiles } from '../hooks/useTauriCommands';
import { TransferProgressPanel } from './TransferProgress';

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
    } catch (e) {
      console.error('选择文件失败:', e);
    }
  };

  // 设备已离线
  if (!device) {
    return (
      <div className="flex flex-col items-center py-14">
        <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">设备已离线</p>
        <button
          onClick={onBack}
          className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 text-sm hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors"
        >
          <ArrowLeft className="w-4 h-4" />
          返回设备列表
        </button>
      </div>
    );
  }

  return (
    <div>
      {/* 设备信息头部 */}
      <div className="flex items-center gap-3 mb-4">
        <button
          onClick={onBack}
          className="p-1.5 rounded-lg hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors flex-shrink-0"
          title="返回设备列表"
        >
          <ArrowLeft className="w-5 h-5 text-slate-500 dark:text-slate-400" />
        </button>

        {/* 设备头像 */}
        <div
          className={`w-9 h-9 rounded-xl bg-gradient-to-br ${deviceColor(device.device_name)} flex items-center justify-center flex-shrink-0`}
        >
          <span className="text-white font-semibold text-sm">
            {deviceInitial(device.device_name)}
          </span>
        </div>

        {/* 设备信息 */}
        <div className="flex-1 min-w-0">
          <p className="text-sm font-semibold text-slate-800 dark:text-slate-200 truncate">
            {device.device_name}
          </p>
          <p className="text-[11px] text-slate-400 dark:text-slate-500">
            {device.ip_address} · 端口 {device.tcp_port}
          </p>
        </div>

        {/* 发送文件按钮 */}
        <button
          onClick={handleSendFile}
          className="p-2 rounded-lg bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 text-amber-500 transition-colors flex-shrink-0"
          title="发送文件"
        >
          <SendHorizonal className="w-4 h-4" />
        </button>
      </div>

      {/* 传输进度（仅该设备） */}
      <TransferProgressPanel deviceId={selectedDeviceId ?? undefined} />
    </div>
  );
}
