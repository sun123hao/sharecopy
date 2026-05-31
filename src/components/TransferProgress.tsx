import { File, CheckCircle2, XCircle, X } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useTransferProgress } from '../hooks/useTauriEvents';
import { cancelTransfer } from '../hooks/useTauriCommands';

/**
 * 传输进度面板
 * - 全局模式（无 deviceId）：显示所有传输，最多 3 条
 * - 设备模式（有 deviceId）：仅显示该设备的传输，全部显示
 */
export function TransferProgressPanel({ deviceId }: { deviceId?: string }) {
  const allTransfers = useAppStore((s) => s.transfers);
  const updateTransferProgress = useAppStore((s) => s.updateTransferProgress);
  const removeTransfer = useAppStore((s) => s.removeTransfer);

  // 按设备过滤（如指定）
  const transfers = deviceId
    ? allTransfers.filter((t) => t.device_id === deviceId)
    : allTransfers;

  // 全局模式显示最近 3 条，设备模式显示全部
  const displayed = deviceId ? transfers : transfers.slice(-3);

  useTransferProgress((p) => {
    updateTransferProgress({
      transfer_id: (p as any).transfer_id ?? p.file_name,
      file_name: p.file_name,
      progress: Math.round(p.progress * 100),
      state: p.state as 'pending' | 'transferring' | 'completed' | 'failed',
      device_id: (p as any).device_id,
    });
  });

  // 全局模式且无传输时隐藏
  if (!deviceId && transfers.length === 0) return null;

  const handleCancel = async (transferId: string) => {
    // 立即从 UI 移除（即时视觉反馈）
    removeTransfer(transferId);
    // 通知后端取消
    try {
      await cancelTransfer(transferId);
    } catch (e) {
      console.error('取消传输失败:', e);
    }
  };

  return (
    <div className="bg-white dark:bg-slate-900 border-t border-slate-200 dark:border-slate-800 px-4 py-3 space-y-2 transition-colors">
      <p className="text-[11px] font-medium text-slate-500 dark:text-slate-400">
        {deviceId ? '传输记录' : '传输进度'}
      </p>

      {displayed.length === 0 ? (
        <p className="text-xs text-slate-400 dark:text-slate-500 py-2 text-center">
          暂无传输记录
        </p>
      ) : (
        <div className="space-y-2 max-h-64 overflow-y-auto">
          {displayed.map((t) => (
            <div
              key={t.transfer_id}
              className="flex items-center gap-3 p-2.5 rounded-xl bg-slate-50 dark:bg-slate-800 border border-slate-100 dark:border-slate-700"
            >
              {t.state === 'completed' ? (
                <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0" />
              ) : t.state === 'failed' ? (
                <XCircle className="w-4 h-4 text-red-400 flex-shrink-0" />
              ) : (
                <File className="w-4 h-4 text-amber-400 flex-shrink-0" />
              )}
              <div className="flex-1 min-w-0">
                <p className="text-xs text-slate-700 dark:text-slate-300 truncate">{t.file_name}</p>
                <div className="mt-1.5 h-1 rounded-full bg-slate-200 dark:bg-slate-700 overflow-hidden">
                  <div
                    className={`h-full rounded-full transition-all duration-300 ${
                      t.state === 'failed' ? 'bg-red-400' : 'bg-amber-500'
                    }`}
                    style={{ width: `${t.progress}%` }}
                  />
                </div>
              </div>
              <span
                className={`text-[10px] font-medium flex-shrink-0 ${
                  t.state === 'completed'
                    ? 'text-emerald-500 dark:text-emerald-400'
                    : t.state === 'failed'
                      ? 'text-red-400'
                      : 'text-slate-400 dark:text-slate-500'
                }`}
              >
                {t.state === 'completed'
                  ? '完成'
                  : t.state === 'failed'
                    ? '失败'
                    : `${t.progress}%`}
              </span>
              {/* 取消按钮：仅传输中显示 */}
              {(t.state === 'transferring' || t.state === 'pending') && (
                <button
                  onClick={() => handleCancel(t.transfer_id)}
                  className="p-1 rounded hover:bg-red-50 dark:hover:bg-red-500/10 text-slate-300 dark:text-slate-600 hover:text-red-400 flex-shrink-0 transition-colors"
                  title="取消传输"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
