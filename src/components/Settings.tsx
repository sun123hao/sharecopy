import { useEffect, useState } from 'react';
import { FolderOpen, Sun, Moon, Monitor } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';
import { isMobile, detectPlatform } from '../hooks/usePlatform';
import { getAndroidSaveDirs } from '../hooks/useTauriCommands';


type Theme = 'system' | 'light' | 'dark';

function Toggle({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      onClick={onChange}
      className={`relative w-10 h-6 rounded-full transition-colors cursor-pointer ${
        checked ? 'bg-amber-500' : 'bg-slate-300 dark:bg-slate-600'
      }`}
    >
      <div
        className={`absolute top-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform ${
          checked ? 'translate-x-[18px]' : 'translate-x-0.5'
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

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  useEffect(() => {
    if (config) {
      setDeviceName(config.device_name);
      setSaveDir(config.save_dir);
      setAutoStart(config.auto_start);
      setAutoAccept(config.auto_accept_files);
    }
  }, [config]);

  const handleDeviceNameSave = () => {
    if (deviceName && deviceName !== config?.device_name) {
      updateDeviceName(deviceName);
    }
  };

  const handleSaveDirChange = (dir: string) => {
    setSaveDir(dir);
    if (config) {
      updateConfigAction({ ...config, save_dir: dir });
    }
  };

  // Android: 加载可用保存目录列表
  const [androidDirs, setAndroidDirs] = useState<string[]>([]);
  const isAndroid = detectPlatform() === 'android';

  useEffect(() => {
    if (isAndroid) {
      getAndroidSaveDirs()
        .then(setAndroidDirs)
        .catch(() => setAndroidDirs([]));
    }
  }, [isAndroid]);

  const handleSelectFolder = async () => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') {
        handleSaveDirChange(selected);
      }
    } catch (e) {
      console.error('选择文件夹失败:', e);
    }
  };

  const handleToggleAutoStart = () => {
    const newVal = !autoStart;
    setAutoStart(newVal);
    if (config) {
      updateConfigAction({ ...config, auto_start: newVal });
    }
  };

  const handleToggleAutoAccept = () => {
    const newVal = !autoAccept;
    setAutoAccept(newVal);
    if (config) {
      updateConfigAction({ ...config, auto_accept_files: newVal });
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-2 mb-4">
        <div className="w-1 h-4 rounded-full bg-amber-500" />
        <h2 className="text-sm font-semibold text-slate-700 dark:text-slate-200">设置</h2>
      </div>

      {/* 设备名称 */}
      <div>
        <label className="text-[11px] text-slate-500 dark:text-slate-400 mb-1.5 block">设备名称</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={deviceName}
            onChange={(e) => setDeviceName(e.target.value)}
            className="flex-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-800 dark:text-slate-200 focus:outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-50 dark:focus:ring-amber-500/10 transition-all"
          />
          {deviceName !== config?.device_name && (
            <button
              onClick={handleDeviceNameSave}
              className="px-3 py-2.5 rounded-xl bg-amber-500 text-white text-sm font-medium hover:bg-amber-600 transition-colors"
            >
              保存
            </button>
          )}
        </div>
      </div>

      {/* 文件保存路径 */}
      <div>
        <label className="text-[11px] text-slate-500 dark:text-slate-400 mb-1.5 block">文件保存路径</label>
        {/* Android: 显示预设目录选项 */}
        {isAndroid && androidDirs.length > 0 ? (
          <div className="space-y-1.5">
            {androidDirs.map((dir) => (
              <button
                key={dir}
                onClick={() => handleSaveDirChange(dir)}
                className={`w-full text-left px-3 py-2 rounded-lg text-xs border transition-colors ${
                  saveDir === dir
                    ? 'border-amber-400 bg-amber-50 dark:bg-amber-500/10 text-amber-700 dark:text-amber-300'
                    : 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-400 hover:border-slate-300 dark:hover:border-slate-600'
                }`}
              >
                {dir}
              </button>
            ))}
            {/* 手动输入 */}
            <input
              type="text"
              value={saveDir}
              onChange={(e) => handleSaveDirChange(e.target.value)}
              placeholder="或手动输入路径..."
              className="w-full bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-lg px-3 py-2 text-xs text-slate-800 dark:text-slate-200 focus:outline-none focus:border-amber-400 transition-all"
            />
          </div>
        ) : (
          <div className="flex gap-2">
            <input
              type="text"
              value={saveDir}
              onChange={(e) => handleSaveDirChange(e.target.value)}
              className="flex-1 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl px-3 py-2.5 text-sm text-slate-800 dark:text-slate-200 focus:outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-50 dark:focus:ring-amber-500/10 transition-all"
            />
            <button
              onClick={handleSelectFolder}
              className="flex items-center justify-center w-10 h-10 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors"
            >
              <FolderOpen className="w-4 h-4 text-slate-400 dark:text-slate-500" />
            </button>
          </div>
        )}
      </div>

      {/* 主题选择 */}
      <div>
        <label className="text-[11px] text-slate-500 dark:text-slate-400 mb-1.5 block">主题</label>
        <div className="flex gap-1.5 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl p-1">
          {themeOptions.map((opt) => {
            const isActive = theme === opt.value;
            const Icon = opt.icon;
            return (
              <button
                key={opt.value}
                onClick={() => setTheme(opt.value)}
                className={`flex-1 flex items-center justify-center gap-1.5 py-2 rounded-lg text-xs transition-colors ${
                  isActive
                    ? 'bg-amber-50 dark:bg-amber-500/15 text-amber-600 dark:text-amber-400'
                    : 'text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-300'
                }`}
              >
                <Icon className="w-3.5 h-3.5" />
                {opt.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* 开关项 */}
      <div className="bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl overflow-hidden">
        {/* 同步开关 */}
        <button
          onClick={() => setSyncEnabled(!syncEnabled)}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors border-b border-slate-100 dark:border-slate-700"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800 dark:text-slate-200">剪贴板同步</p>
            <p className="text-[11px] text-slate-400 dark:text-slate-500 mt-0.5">
              自动同步文本和图片到其他设备
            </p>
          </div>
          <Toggle checked={syncEnabled} onChange={() => setSyncEnabled(!syncEnabled)} />
        </button>

        {/* 开机自启（桌面端专属） */}
        {!isMobile() && (
        <button
          onClick={handleToggleAutoStart}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors border-b border-slate-100 dark:border-slate-700"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800 dark:text-slate-200">开机自启</p>
            <p className="text-[11px] text-slate-400 dark:text-slate-500 mt-0.5">
              登录系统时自动启动 ShareCopy
            </p>
          </div>
          <Toggle checked={autoStart} onChange={handleToggleAutoStart} />
        </button>
        )}

        {/* 自动接收 */}
        <button
          onClick={handleToggleAutoAccept}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800 dark:text-slate-200">自动接收文件</p>
            <p className="text-[11px] text-slate-400 dark:text-slate-500 mt-0.5">
              无需确认直接保存接收的文件
            </p>
          </div>
          <Toggle checked={autoAccept} onChange={handleToggleAutoAccept} />
        </button>
      </div>

      {/* 同步统计 */}
      <div className="pt-4 border-t border-slate-200 dark:border-slate-800">
        <p className="text-[11px] text-slate-500 dark:text-slate-400 mb-2">同步统计</p>
        <StatsSection />
      </div>

      {/* 版本信息 */}
      <div className="pt-4 border-t border-slate-200 dark:border-slate-800">
        <p className="text-[11px] text-slate-400 dark:text-slate-500">
          ShareCopy v0.1.0 · Tauri + Rust + React
        </p>
      </div>
    </div>
  );
}

// 同步统计子组件
function StatsSection() {
  const stats = useAppStore((s) => s.stats);
  const refreshStats = useAppStore((s) => s.refreshStats);

  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  return (
    <div className="grid grid-cols-3 gap-2 text-center">
      <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
        <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.texts_synced}</p>
        <p className="text-[10px] text-slate-400 dark:text-slate-500 mt-0.5">文本</p>
      </div>
      <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
        <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.images_synced}</p>
        <p className="text-[10px] text-slate-400 dark:text-slate-500 mt-0.5">图片</p>
      </div>
      <div className="p-3 rounded-xl bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700">
        <p className="text-lg font-bold text-slate-800 dark:text-slate-200">{stats.files_transferred}</p>
        <p className="text-[10px] text-slate-400 dark:text-slate-500 mt-0.5">文件</p>
      </div>
    </div>
  );
}
