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
    try { setEntries(await getClipboardHistory()); } catch (e) { console.error('加载失败:', e); }
  };
  useEffect(() => { refreshStats(); loadHistory(); }, [refreshStats]);
  useClipboardUpdated(() => { refreshStats(); loadHistory(); });

  const filteredEntries = useMemo(() => {
    if (!search.trim()) return entries;
    const q = search.toLowerCase();
    return entries.filter((e) =>
      (e.type === 'text' && e.content.toLowerCase().includes(q)) || e.from_device.toLowerCase().includes(q)
    );
  }, [entries, search]);

  const handleCopy = async (entry: ClipboardEntry) => {
    try { await copyFromHistory(entry.id); setCopiedId(entry.id); setTimeout(() => setCopiedId(null), 2000); }
    catch (e) { console.error('重新复制失败:', e); }
  };

  const formatTime = (ts: number): string => {
    const diff = Date.now() - ts;
    if (diff < 60_000) return '刚刚';
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    return new Date(ts).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
  };

  const truncateText = (text: string, maxLen = 80): string => {
    const singleLine = text.replace(/\n/g, ' ');
    return singleLine.length > maxLen ? singleLine.slice(0, maxLen) + '...' : singleLine;
  };

  return (
    <div>
      {/* 统计卡片 */}
      <div className="grid grid-cols-3 gap-2 mb-5">
        {[
          { n: stats.texts_synced, label: '条文本', icon: FileText, color: 'text-accent' },
          { n: stats.images_synced, label: '张图片', icon: ImageIcon, color: 'text-ios-purple' },
          { n: stats.files_transferred, label: '个文件', icon: File, color: 'text-ios-green' },
        ].map((s) => {
          const Icon = s.icon;
          return (
            <div key={s.label} className="p-3 rounded-[16px] glass-thin border border-white/10 text-center">
              <Icon className={`w-[18px] h-[18px] ${s.color} mx-auto mb-1.5`} />
              <p className="text-[20px] font-semibold">{s.n}</p>
              <p className="text-[11px] text-black/40 dark:text-white/40">{s.label}</p>
            </div>
          );
        })}
      </div>

      {entries.length === 0 ? (
        <div className="text-center py-16">
          <div className="w-[68px] h-[68px] mx-auto mb-4 rounded-[20px] glass-thin border border-white/10 flex items-center justify-center">
            <ClipboardList className="w-7 h-7 text-black/15 dark:text-white/15" />
          </div>
          <p className="text-[15px] text-black/60 dark:text-white/60">暂无同步记录</p>
          <p className="text-[13px] text-black/30 dark:text-white/30 mt-1.5">同步过的文本和图片将显示在这里</p>
        </div>
      ) : (
        <>
          {/* 搜索栏 */}
          {entries.length > 5 && (
            <div className="relative mb-4">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-[15px] h-[15px] text-black/20 dark:text-white/20" />
              <input
                type="text" placeholder="搜索历史…" value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full pl-9 pr-8 py-2.5 text-[15px] rounded-[14px] glass-thin border border-white/10 bg-transparent text-black dark:text-white placeholder:text-black/20 dark:placeholder:text-white/20 focus:outline-none focus:border-accent/40 transition-all"
              />
              {search && (
                <button onClick={() => setSearch('')} className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1 rounded-[6px] hover:bg-black/5 dark:hover:bg-white/5">
                  <X className="w-[15px] h-[15px] text-black/30 dark:text-white/30" />
                </button>
              )}
            </div>
          )}

          <div className="space-y-[7px]">
            {filteredEntries.length === 0 ? (
              <p className="text-center text-[13px] text-black/30 dark:text-white/30 py-8">无匹配记录</p>
            ) : (
              filteredEntries.map((entry) => (
                <div key={entry.id}
                  className="group flex items-start gap-3 p-3 rounded-[16px] glass-thin border border-white/10 hover:border-accent/20 transition-all duration-200"
                >
                  <div className="flex-shrink-0 mt-0.5">
                    {entry.type === 'text'
                      ? <FileText className="w-[18px] h-[18px] text-accent" />
                      : <ImageIcon className="w-[18px] h-[18px] text-ios-purple" />}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-[13px] leading-relaxed break-all">
                      {entry.type === 'text' ? truncateText(entry.content) : '[图片]'}
                    </p>
                    <div className="flex items-center gap-2 mt-1.5">
                      <span className="text-[11px] text-black/30 dark:text-white/30">{entry.from_device}</span>
                      <span className="text-[11px] text-black/15 dark:text-white/15">·</span>
                      <span className="text-[11px] text-black/20 dark:text-white/20">{formatTime(entry.timestamp)}</span>
                    </div>
                  </div>
                  <button
                    onClick={() => handleCopy(entry)}
                    className={`flex-shrink-0 p-1.5 rounded-[8px] transition-all duration-200 opacity-0 group-hover:opacity-100 touch-visible min-touch-target ${
                      copiedId === entry.id
                        ? 'bg-ios-green/10 text-ios-green'
                        : 'hover:bg-accent/10 text-black/15 dark:text-white/15 hover:text-accent'
                    }`}
                    title="重新复制"
                  ><Copy className="w-[14px] h-[14px]" /></button>
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  );
}
