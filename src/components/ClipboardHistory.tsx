import { useEffect } from 'react';
import { ClipboardList, FileText, Image as ImageIcon, File } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useClipboardUpdated } from '../hooks/useTauriEvents';

export function ClipboardHistory() {
  const stats = useAppStore((s) => s.stats);
  const refreshStats = useAppStore((s) => s.refreshStats);

  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  useClipboardUpdated(() => {
    refreshStats();
  });

  const hasActivity = stats.texts_synced > 0 || stats.images_synced > 0 || stats.files_transferred > 0;

  return (
    <div>
      <div className="flex items-center gap-2 mb-4">
        <div className="w-1 h-4 rounded-full bg-amber-500" />
        <h2 className="text-sm font-semibold text-slate-700">剪贴板历史</h2>
      </div>

      {!hasActivity ? (
        <div className="text-center py-14">
          <div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-slate-100 flex items-center justify-center">
            <ClipboardList className="w-7 h-7 text-slate-300" />
          </div>
          <p className="text-sm text-slate-500">暂无同步记录</p>
          <p className="text-xs text-slate-400 mt-1.5">
            同步过的文本和图片将显示在这里
          </p>
        </div>
      ) : (
        <div className="space-y-3">
          {/* 统计概览 */}
          <div className="grid grid-cols-3 gap-2">
            <div className="p-3 rounded-xl bg-white border border-slate-200 text-center">
              <FileText className="w-4 h-4 text-amber-400 mx-auto mb-1" />
              <p className="text-lg font-bold text-slate-800">{stats.texts_synced}</p>
              <p className="text-[10px] text-slate-400">条文本</p>
            </div>
            <div className="p-3 rounded-xl bg-white border border-slate-200 text-center">
              <ImageIcon className="w-4 h-4 text-blue-400 mx-auto mb-1" />
              <p className="text-lg font-bold text-slate-800">{stats.images_synced}</p>
              <p className="text-[10px] text-slate-400">张图片</p>
            </div>
            <div className="p-3 rounded-xl bg-white border border-slate-200 text-center">
              <File className="w-4 h-4 text-emerald-400 mx-auto mb-1" />
              <p className="text-lg font-bold text-slate-800">{stats.files_transferred}</p>
              <p className="text-[10px] text-slate-400">个文件</p>
            </div>
          </div>

          <p className="text-[11px] text-slate-400 text-center pt-2">
            详细历史记录将在后续版本中支持
          </p>
        </div>
      )}
    </div>
  );
}
