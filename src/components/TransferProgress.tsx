import { File, CheckCircle2, XCircle } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useTransferProgress } from '../hooks/useTauriEvents';

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
    <div className="bg-white border-t border-slate-200 px-4 py-3 space-y-2">
      <p className="text-[11px] font-medium text-slate-500">传输进度</p>
      {transfers.slice(-3).map((t) => (
        <div
          key={t.file_name}
          className="flex items-center gap-3 p-2.5 rounded-xl bg-slate-50 border border-slate-100"
        >
          {t.state === 'completed' ? (
            <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0" />
          ) : t.state === 'failed' ? (
            <XCircle className="w-4 h-4 text-red-400 flex-shrink-0" />
          ) : (
            <File className="w-4 h-4 text-amber-400 flex-shrink-0" />
          )}
          <div className="flex-1 min-w-0">
            <p className="text-xs text-slate-700 truncate">{t.file_name}</p>
            <div className="mt-1.5 h-1 rounded-full bg-slate-200 overflow-hidden">
              <div
                className={`h-full rounded-full transition-all duration-300 ${
                  t.state === 'failed' ? 'bg-red-400' : 'bg-amber-500'
                }`}
                style={{ width: `${t.progress}%` }}
              />
            </div>
          </div>
          <span
            className={`text-[10px] font-medium ${
              t.state === 'completed'
                ? 'text-emerald-500'
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
