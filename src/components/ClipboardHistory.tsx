import { useEffect, useState, useMemo } from 'react';
import { ClipboardList, FileText, Image as ImageIcon, File, Copy, Search, X } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { useClipboardUpdated } from '../hooks/useTauriEvents';
import { getClipboardHistory, copyFromHistory } from '../hooks/useTauriCommands';
import type { ClipboardEntry } from '../types';

export function ClipboardHistory() {
  const stats = useAppStore((s) => s.stats);
  const refreshStats = useAppStore((s) => s.refreshStats);

  const [entries, setEntries] = useState<ClipboardEntry[]>([]);
  const [search, setSearch] = useState('');
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const loadHistory = async () => {
    try {
      const result = await getClipboardHistory();
      setEntries(result);
    } catch (e) {
      console.error('加载剪贴板历史失败:', e);
    }
  };

  useEffect(() => {
    refreshStats();
    loadHistory();
  }, [refreshStats]);

  useClipboardUpdated(() => {
    refreshStats();
    loadHistory();
  });

  const filteredEntries = useMemo(() => {
    if (!search.trim()) return entries;
    const q = search.toLowerCase();
    return entries.filter(
      (e) =>
        (e.type === 'text' && e.content.toLowerCase().includes(q)) ||
        e.from_device.toLowerCase().includes(q)
    );
  }, [entries, search]);

  const handleCopy = async (entry: ClipboardEntry) => {
    try {
      await copyFromHistory(entry.id);
      setCopiedId(entry.id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch (e) {
      console.error('重新复制失败:', e);
    }
  };

  const formatTime = (ts: number): string => {
    const d = new Date(ts);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 60_000) return '刚刚';
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    return d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
  };

  const truncateText = (text: string, maxLen = 80): string => {
    const singleLine = text.replace(/\n/g, ' ');
    return singleLine.length > maxLen ? singleLine.slice(0, maxLen) + '...' : singleLine;
  };

  const hasActivity = entries.length > 0;

  return (
    <div>
      <div className="flex items-center gap-2 mb-4">
        <div className="w-1 h-4 rounded-full bg-amber-500" />
        <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">剪贴板历史</h2>
      </div>

      {/* 统计概览 */}
      <div className="grid grid-cols-3 gap-2 mb-4">
        <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-center">
          <FileText className="w-4 h-4 text-amber-400 mx-auto mb-1" />
          <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.texts_synced}</p>
          <p className="text-[10px] text-slate-400 dark:text-slate-500">条文本</p>
        </div>
        <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-center">
          <ImageIcon className="w-4 h-4 text-blue-400 mx-auto mb-1" />
          <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.images_synced}</p>
          <p className="text-[10px] text-slate-400 dark:text-slate-500">张图片</p>
        </div>
        <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-center">
          <File className="w-4 h-4 text-emerald-400 mx-auto mb-1" />
          <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.files_transferred}</p>
          <p className="text-[10px] text-slate-400 dark:text-slate-500">个文件</p>
        </div>
      </div>

      {!hasActivity ? (
        <div className="text-center py-10">
          <div className="w-16 h-16 mx-auto mb-4 rounded-2xl bg-slate-100 dark:bg-slate-800 flex items-center justify-center">
            <ClipboardList className="w-7 h-7 text-slate-300 dark:text-slate-600" />
          </div>
          <p className="text-sm text-slate-500 dark:text-slate-400">暂无同步记录</p>
          <p className="text-xs text-slate-400 dark:text-slate-500 mt-1.5">
            同步过的文本和图片将显示在这里
          </p>
        </div>
      ) : (
        <>
          {/* 搜索栏 */}
          {entries.length > 5 && (
            <div className="relative mb-3">
              <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-400" />
              <input
                type="text"
                placeholder="搜索历史..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full pl-8 pr-7 py-1.5 text-xs rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-800 dark:text-slate-200 placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:border-amber-300 dark:focus:border-amber-500 focus:ring-1 focus:ring-amber-100 dark:focus:ring-amber-500/10 transition-colors"
              />
              {search && (
                <button
                  onClick={() => setSearch('')}
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded hover:bg-slate-100"
                >
                  <X className="w-3 h-3 text-slate-400" />
                </button>
              )}
            </div>
          )}

          {/* 历史列表 */}
          <div className="space-y-1.5">
            {filteredEntries.length === 0 ? (
              <p className="text-center text-xs text-slate-400 dark:text-slate-500 py-6">无匹配记录</p>
            ) : (
              filteredEntries.map((entry) => (
                <div
                  key={entry.id}
                  className="group flex items-start gap-2.5 p-2.5 rounded-lg hover:bg-white dark:hover:bg-slate-800 hover:shadow-sm border border-transparent hover:border-slate-200 dark:hover:border-slate-700 transition-all"
                >
                  {/* 类型图标 */}
                  <div className="flex-shrink-0 mt-0.5">
                    {entry.type === 'text' ? (
                      <FileText className="w-4 h-4 text-amber-400" />
                    ) : (
                      <ImageIcon className="w-4 h-4 text-blue-400" />
                    )}
                  </div>

                  {/* 内容 */}
                  <div className="flex-1 min-w-0">
                    <p className="text-xs text-slate-700 dark:text-slate-300 leading-relaxed break-all">
                      {entry.type === 'text'
                        ? truncateText(entry.content)
                        : '[图片]'}
                    </p>
                    <div className="flex items-center gap-2 mt-1">
                      <span className="text-[10px] text-slate-400">
                        {entry.from_device}
                      </span>
                      <span className="text-[10px] text-slate-300">·</span>
                      <span className="text-[10px] text-slate-300">
                        {formatTime(entry.timestamp)}
                      </span>
                    </div>
                  </div>

                  {/* 重新复制按钮（桌面端 hover 显示，触控端始终可见） */}
                  <button
                    onClick={() => handleCopy(entry)}
                    className={`flex-shrink-0 p-1 rounded-md transition-all opacity-0 group-hover:opacity-100 touch-visible min-touch-target ${
                      copiedId === entry.id
                        ? 'bg-emerald-100 text-emerald-600'
                        : 'hover:bg-amber-50 text-slate-300 hover:text-amber-500'
                    }`}
                    title="重新复制"
                  >
                    <Copy className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}
