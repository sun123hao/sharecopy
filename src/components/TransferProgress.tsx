import { CheckCircle2, XCircle, X, FolderOpen } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useTransferProgress } from '../hooks/useTauriEvents';
import { cancelTransfer, openFileDir } from '../hooks/useTauriCommands';
import { formatTime } from '../utils/time';

export function TransferProgressPanel({ deviceId }: { deviceId?: string }) {
  const allTransfers = useAppStore((s) => s.transfers);
  const updateTransferProgress = useAppStore((s) => s.updateTransferProgress);
  const removeTransfer = useAppStore((s) => s.removeTransfer);

  const transfers = deviceId
    ? allTransfers.filter((t) => t.device_id === deviceId)
    : allTransfers;
  const displayed = deviceId ? transfers : transfers.slice(-1);

  useTransferProgress((p) => {
    updateTransferProgress({
      transfer_id: (p as any).transfer_id ?? p.file_name,
      file_name: p.file_name,
      progress: Math.round(p.progress * 100),
      state: p.state as 'pending' | 'transferring' | 'completed' | 'failed',
      device_id: (p as any).device_id,
      timestamp: (p as any).timestamp ?? Date.now(),
      save_path: (p as any).save_path,
    });
  });

  if (!deviceId && transfers.length === 0) return null;

  const handleCancel = async (transferId: string) => {
    removeTransfer(transferId);
    try { await cancelTransfer(transferId); } catch (e) { console.error('取消传输失败:', e); }
  };

  const handleDismiss = (transferId: string) => {
    removeTransfer(transferId);
  };

  const handleOpenDir = async (savePath?: string) => {
    if (!savePath) return;
    try { await openFileDir(savePath); } catch (e) { console.error('打开目录失败:', e); }
  };

  return (
    <div className={`border-t border-ios-separator dark:border-ios-separator-dark px-4 py-3 transition-colors ${deviceId ? 'flex-1 min-h-0 flex flex-col bg-ios-card dark:bg-ios-card-dark' : 'glass-thick flex-shrink-0 space-y-[7px]'}`}>
      <p className="text-[11px] font-medium text-black/40 dark:text-white/40 uppercase tracking-[0.02em] mb-[7px] flex-shrink-0">
        {deviceId ? '传输记录' : '传输进度'}
      </p>

      {displayed.length === 0 ? (
        <p className="text-[13px] text-black/20 dark:text-white/20 py-2 text-center">
          暂无传输记录
        </p>
      ) : (
        <div className={`space-y-[7px] overflow-y-auto ${deviceId ? 'flex-1 min-h-0' : 'max-h-64'}`}>
          {displayed.map((t) => (
            <div
              key={t.transfer_id}
              className="flex items-center gap-[10px] p-[11px] rounded-[14px] ios-card border border-white/10"
            >
              {t.state === 'completed' ? (
                <CheckCircle2 className="w-[18px] h-[18px] text-ios-green flex-shrink-0" />
              ) : t.state === 'failed' ? (
                <XCircle className="w-[18px] h-[18px] text-ios-red flex-shrink-0" />
              ) : (
                <div className="w-[18px] h-[18px] rounded-[5px] bg-accent/15 flex items-center justify-center flex-shrink-0">
                  <div className="w-2 h-2 rounded-sm bg-accent" />
                </div>
              )}
              <div className="flex-1 min-w-0">
                <p className="text-[13px] truncate">{t.file_name}</p>
                {(t as any).timestamp && (
                  <p className="text-[10px] text-black/30 dark:text-white/30 mt-0.5">
                    {formatTime((t as any).timestamp)}
                  </p>
                )}
                <div className="h-1 rounded-full bg-black/10 dark:bg-white/10 overflow-hidden mt-1.5">
                  <div
                    className={`h-full rounded-full transition-all duration-400 ${
                      t.state === 'failed' ? 'bg-ios-red' : t.state === 'completed' ? 'bg-ios-green' : 'bg-accent'
                    }`}
                    style={{ width: `${t.progress}%` }}
                  />
                </div>
              </div>
              <span
                className={`text-[11px] font-medium flex-shrink-0 min-w-[28px] text-right ${
                  t.state === 'completed' ? 'text-ios-green'
                    : t.state === 'failed' ? 'text-ios-red'
                    : 'text-black/40 dark:text-white/40'
                }`}
              >
                {t.state === 'completed' ? '完成' : t.state === 'failed' ? '失败' : `${t.progress}%`}
              </span>
              {(t.state === 'completed' || t.state === 'failed') && (t as any).save_path && (
                <button
                  onClick={() => handleOpenDir((t as any).save_path)}
                  className="w-6 h-6 rounded-full flex items-center justify-center text-black/20 dark:text-white/20 hover:bg-accent/10 hover:text-accent transition-all duration-200 flex-shrink-0"
                  title="打开文件目录"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                </button>
              )}
              {(t.state === 'transferring' || t.state === 'pending') ? (
                <button
                  onClick={() => handleCancel(t.transfer_id)}
                  className="w-6 h-6 rounded-full flex items-center justify-center text-black/20 dark:text-white/20 hover:bg-ios-red/10 hover:text-ios-red transition-all duration-200 flex-shrink-0"
                  title="取消传输"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              ) : (
                <button
                  onClick={() => handleDismiss(t.transfer_id)}
                  className="w-6 h-6 rounded-full flex items-center justify-center text-black/15 dark:text-white/15 hover:bg-black/5 dark:hover:bg-white/5 hover:text-black/40 dark:hover:text-white/40 transition-all duration-200 flex-shrink-0"
                  title="移除记录"
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
