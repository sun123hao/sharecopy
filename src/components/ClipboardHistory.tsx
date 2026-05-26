import { useEffect } from 'react';
import { FileText } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useClipboardUpdated } from '../hooks/useTauriEvents';

export function ClipboardHistory() {
  const stats = useAppStore((s) => s.stats);
  const refreshStats = useAppStore((s) => s.refreshStats);

  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  // 监听剪贴板更新事件
  useClipboardUpdated((_entry) => {
    refreshStats();
  });

  const hasActivity = stats.texts_synced > 0 || stats.images_synced > 0;

  return (
    <div>
      <h2 className="text-sm font-medium text-slate-400 mb-4">剪贴板历史</h2>
      {!hasActivity ? (
        <div className="text-center py-12">
          <p className="text-slate-500">暂无同步记录</p>
          <p className="text-xs text-slate-600 mt-2">
            同步过的文本和图片将显示在这里
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          <div className="flex items-center gap-3 p-3 rounded-lg bg-slate-900 border border-slate-800">
            <FileText className="w-4 h-4 text-slate-400 flex-shrink-0" />
            <div className="flex-1 min-w-0">
              <p className="text-xs text-slate-500">
                已同步 <span className="text-white font-medium">{stats.texts_synced}</span> 条文本
                ，<span className="text-white font-medium">{stats.images_synced}</span> 张图片
                ，<span className="text-white font-medium">{stats.files_transferred}</span> 个文件
              </p>
            </div>
          </div>
          <p className="text-xs text-slate-600 text-center">
            详细历史记录将在后续版本中支持
          </p>
        </div>
      )}
    </div>
  );
}
