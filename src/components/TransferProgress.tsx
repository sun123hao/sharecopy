import { useAppStore } from '../store/useAppStore';
import { useTransferProgress } from '../hooks/useTauriEvents';
import { File } from 'lucide-react';

export function TransferProgressPanel() {
  const transfers = useAppStore((s) => s.transfers);
  const updateTransferProgress = useAppStore((s) => s.updateTransferProgress);

  useTransferProgress((p) => {
    updateTransferProgress({
      file_name: p.file_name,
      progress: Math.round(p.progress * 100),
      state: p.state as 'pending' | 'transferring' | 'completed' | 'failed',
    });
  });

  if (transfers.length === 0) return null;

  return (
    <div className="border-t border-slate-800 px-4 py-3 space-y-2">
      <p className="text-xs font-medium text-slate-400">传输进度</p>
      {transfers.slice(-3).map((t) => (
        <div
          key={t.file_name}
          className="flex items-center gap-3 p-2 rounded bg-slate-900/50"
        >
          <File className="w-4 h-4 text-slate-400 flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <p className="text-xs text-white truncate">{t.file_name}</p>
            <div className="mt-1 h-1 rounded-full bg-slate-700 overflow-hidden">
              <div
                className={`h-full rounded-full transition-all duration-300 ${
                  t.state === 'failed' ? 'bg-red-500' : 'bg-blue-500'
                }`}
                style={{ width: `${t.progress}%` }}
              />
            </div>
          </div>
          <span
            className={`text-xs ${
              t.state === 'completed'
                ? 'text-green-400'
                : t.state === 'failed'
                  ? 'text-red-400'
                  : 'text-slate-400'
            }`}
          >
            {t.state === 'completed'
              ? '完成'
              : t.state === 'failed'
                ? '失败'
                : `${t.progress}%`}
          </span>
        </div>
      ))}
    </div>
  );
}
