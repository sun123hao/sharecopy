import { useEffect, useState } from 'react';
import { FolderOpen, Sun, Moon, Monitor } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { isMobile, detectPlatform } from '../hooks/usePlatform';
import { getAndroidSaveDirs } from '../hooks/useTauriCommands';

type Theme = 'system' | 'light' | 'dark';

/* iOS 18+ Toggle 开关 */
function Toggle({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      onClick={onChange}
      className={`relative w-[51px] h-[31px] rounded-[16px] transition-colors duration-300 flex-shrink-0 border ${
        checked ? 'bg-ios-green border-transparent' : 'bg-black/[0.12] dark:bg-white/[0.16] border-black/[0.04] dark:border-white/[0.04]'
      }`}
    >
      <div
        className={`absolute top-[1px] w-[27px] h-[27px] bg-white rounded-full shadow-[0_3px_8px_rgba(0,0,0,0.15)] transition-transform duration-300 ${
          checked ? 'translate-x-[22px]' : 'translate-x-[1px]'
        }`}
      />
    </button>
  );
}

const themeOptions: { value: Theme; label: string; icon: typeof Sun }[] = [
  { value: 'system', label: '跟随系统', icon: Monitor },
  { value: 'light', label: '浅色', icon: Sun },
  { value: 'dark', label: '深色', icon: Moon },
];

export function Settings({ theme, setTheme }: { theme: Theme; setTheme: (t: Theme) => void }) {
  const config = useAppStore((s) => s.config);
  const syncEnabled = useAppStore((s) => s.syncEnabled);
  const loadConfig = useAppStore((s) => s.loadConfig);
  const setSyncEnabled = useAppStore((s) => s.setSyncEnabled);
  const updateDeviceName = useAppStore((s) => s.updateDeviceName);
  const updateConfigAction = useAppStore((s) => s.updateConfig);

  const [deviceName, setDeviceName] = useState('');
  const [saveDir, setSaveDir] = useState('');
  const [autoStart, setAutoStart] = useState(false);
  const [autoAccept, setAutoAccept] = useState(true);

  useEffect(() => { loadConfig(); }, [loadConfig]);

  useEffect(() => {
    if (config) {
      setDeviceName(config.device_name);
      setSaveDir(config.save_dir);
      setAutoStart(config.auto_start);
      setAutoAccept(config.auto_accept_files);
    }
  }, [config]);

  const handleSaveDirChange = (dir: string) => {
    setSaveDir(dir);
    if (config) updateConfigAction({ ...config, save_dir: dir });
  };

  const [androidDirs, setAndroidDirs] = useState<string[]>([]);
  const isAndroid = detectPlatform() === 'android';
  useEffect(() => {
    if (isAndroid) getAndroidSaveDirs().then(setAndroidDirs).catch(() => setAndroidDirs([]));
  }, [isAndroid]);

  const handleSelectFolder = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') handleSaveDirChange(selected);
    } catch (e) { console.error('选择文件夹失败:', e); }
  };

  const handleToggleAutoStart = () => {
    const newVal = !autoStart; setAutoStart(newVal);
    if (config) updateConfigAction({ ...config, auto_start: newVal });
  };
  const handleToggleAutoAccept = () => {
    const newVal = !autoAccept; setAutoAccept(newVal);
    if (config) updateConfigAction({ ...config, auto_accept_files: newVal });
  };

  /* ── iOS 18+ 分组卡片样式 ── */
  const groupClass = "rounded-[20px] ios-card border border-white/10 dark:border-white/5 overflow-hidden mb-6 shadow-[0_1px_2px_rgba(0,0,0,0.04)]";
  const groupHeaderClass = "text-[11px] font-medium text-black/40 dark:text-white/40 uppercase tracking-[0.02em] px-[18px] pt-[14px] pb-[6px]";
  const rowClass = "flex items-center justify-between px-[18px] py-[13px] border-b border-ios-separator dark:border-ios-separator-dark last:border-b-0";
  const inputClass = "w-full rounded-[10px] ios-card border border-white/10 dark:border-white/5 px-3 py-2.5 text-[15px] bg-transparent text-black dark:text-white placeholder:text-black/20 dark:placeholder:text-white/20 focus:outline-none focus:border-accent/40 transition-all";

  return (
    <div>
      {/* 设备信息 */}
      <div className={groupClass}>
        <div className={groupHeaderClass}>设备信息</div>
        <div className={rowClass}>
          <span className="text-[15px]">名称</span>
          <div className="flex items-center gap-2">
            <input
              type="text" value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              placeholder="输入设备名称"
              className="text-right text-[15px] bg-transparent text-black dark:text-white placeholder:text-black/20 dark:placeholder:text-white/20 focus:outline-none w-32"
            />
            <button
              onClick={() => { if (deviceName && deviceName !== config?.device_name) updateDeviceName(deviceName); }}
              className="text-[13px] text-accent font-medium hover:opacity-70 transition-opacity"
            >保存</button>
          </div>
        </div>
        {config && (
          <div className={rowClass}>
            <span className="text-[15px]">信息</span>
            <span className="text-[13px] text-black/40 dark:text-white/40">
              端口 {config.tcp_port} · {config.device_id.slice(0, 8)}…
              <span className="ml-2 text-[11px] text-ios-green bg-ios-green/10 px-2 py-0.5 rounded-full border border-ios-green/20">在线</span>
            </span>
          </div>
        )}
      </div>

      {/* 保存路径 */}
      <div className={groupClass}>
        <div className={groupHeaderClass}>文件保存位置</div>
        <div className="px-[18px] py-[13px]">
          {isAndroid && androidDirs.length > 0 ? (
            <div className="space-y-2">
              <div className="flex gap-2 flex-wrap">
                {androidDirs.map((dir) => (
                  <button
                    key={dir}
                    onClick={() => handleSaveDirChange(dir)}
                    className={`text-[12px] px-3 py-1.5 rounded-[10px] border transition-all duration-200 ${
                      saveDir === dir
                        ? 'bg-accent text-white border-transparent shadow-[0_2px_6px_rgba(0,122,255,0.25)]'
                        : 'ios-card border-ios-separator dark:border-white/5 text-black/60 dark:text-white/60'
                    }`}
                  >{dir}</button>
                ))}
              </div>
              <input
                type="text" value={saveDir}
                onChange={(e) => handleSaveDirChange(e.target.value)}
                placeholder="或手动输入路径…"
                className={inputClass + ' text-[13px]'}
              />
            </div>
          ) : (
            <div className="flex gap-2">
              <input
                type="text" value={saveDir}
                onChange={(e) => handleSaveDirChange(e.target.value)}
                className={`flex-1 ${inputClass}`}
              />
              <button
                onClick={handleSelectFolder}
                className="w-10 h-10 rounded-[10px] ios-card border border-white/10 flex items-center justify-center text-black/30 dark:text-white/30 hover:text-accent transition-colors flex-shrink-0"
              ><FolderOpen className="w-[18px] h-[18px]" /></button>
            </div>
          )}
        </div>
      </div>

      {/* 同步与开关 */}
      <div className={groupClass}>
        <div className={groupHeaderClass}>同步与开关</div>
        <button onClick={() => setSyncEnabled(!syncEnabled)} className={`w-full text-left ${rowClass} hover:bg-black/[0.02] dark:hover:bg-white/[0.02] transition-colors`}>
          <div><p className="text-[15px]">剪贴板同步</p><p className="text-[11px] text-black/40 dark:text-white/40 mt-0.5">自动同步文本和图片到其他设备</p></div>
          <Toggle checked={syncEnabled} onChange={() => setSyncEnabled(!syncEnabled)} />
        </button>
        {!isMobile() && (
        <button onClick={handleToggleAutoStart} className={`w-full text-left ${rowClass} hover:bg-black/[0.02] dark:hover:bg-white/[0.02] transition-colors`}>
          <div><p className="text-[15px]">开机自启</p><p className="text-[11px] text-black/40 dark:text-white/40 mt-0.5">登录系统时自动启动 ShareCopy</p></div>
          <Toggle checked={autoStart} onChange={handleToggleAutoStart} />
        </button>
        )}
        <button onClick={handleToggleAutoAccept} className={`w-full text-left ${rowClass} hover:bg-black/[0.02] dark:hover:bg-white/[0.02] transition-colors`}>
          <div><p className="text-[15px]">自动接收文件</p><p className="text-[11px] text-black/40 dark:text-white/40 mt-0.5">无需确认直接保存接收的文件</p></div>
          <Toggle checked={autoAccept} onChange={handleToggleAutoAccept} />
        </button>
      </div>

      {/* 主题 */}
      <div className={groupClass}>
        <div className={groupHeaderClass}>外观</div>
        <div className={rowClass} style={{ flexDirection: 'column', alignItems: 'flex-start', gap: 10 }}>
          <span className="text-[15px]">主题</span>
          <div className="flex gap-2">
            {themeOptions.map((opt) => {
              const isActive = theme === opt.value;
              const Icon = opt.icon;
              return (
                <button
                  key={opt.value}
                  onClick={() => setTheme(opt.value)}
                  className={`flex items-center gap-1.5 px-4 py-2 rounded-[8px] text-[13px] font-medium transition-all duration-200 ${
                    isActive
                      ? 'bg-accent text-white shadow-[0_2px_8px_rgba(0,122,255,0.3)]'
                      : 'ios-card border border-white/10 text-black/60 dark:text-white/60 hover:text-black dark:hover:text-white'
                  }`}
                ><Icon className="w-[14px] h-[14px]" />{opt.label}</button>
              );
            })}
          </div>
        </div>
      </div>

      {/* 统计 */}
      <div className={groupClass}>
        <div className={groupHeaderClass}>同步统计</div>
        <StatsSection />
      </div>

      {/* 版本 */}
      <div className="text-center pb-6">
        <p className="text-[11px] text-black/20 dark:text-white/20">ShareCopy v0.1.0 · Tauri + Rust + React</p>
      </div>
    </div>
  );
}

function StatsSection() {
  const stats = useAppStore((s) => s.stats);
  const refreshStats = useAppStore((s) => s.refreshStats);
  useEffect(() => { refreshStats(); }, [refreshStats]);
  return (
    <div className="grid grid-cols-3 gap-2 px-[18px] pb-[14px] pt-2">
      {[
        { n: stats.texts_synced, label: '文本', icon: 'T' },
        { n: stats.images_synced, label: '图片', icon: '􀢷' },
        { n: stats.files_transferred, label: '文件', icon: '􀈓' },
      ].map((s) => (
        <div key={s.label} className="p-3 rounded-[14px] ios-card border border-white/10 text-center">
          <p className="text-[20px] font-semibold">{s.n}</p>
          <p className="text-[11px] text-black/40 dark:text-white/40 mt-0.5">{s.label}</p>
        </div>
      ))}
    </div>
  );
}
