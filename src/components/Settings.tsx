import { useEffect, useState } from 'react';
import { FolderOpen } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';

function Toggle({ checked, onChange }: { checked: boolean; onChange: () => void }) {
  return (
    <button
      onClick={onChange}
      className={`relative w-10 h-6 rounded-full transition-colors cursor-pointer ${
        checked ? 'bg-amber-500' : 'bg-slate-300'
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

export function Settings() {
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

  const handleDeviceNameBlur = () => {
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
        <h2 className="text-sm font-semibold text-slate-700">设置</h2>
      </div>

      {/* 设备名称 */}
      <div>
        <label className="text-[11px] text-slate-500 mb-1.5 block">设备名称</label>
        <input
          type="text"
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          onBlur={handleDeviceNameBlur}
          className="w-full bg-white border border-slate-200 rounded-xl px-3 py-2.5 text-sm text-slate-800 focus:outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-50 transition-all"
        />
      </div>

      {/* 文件保存路径 */}
      <div>
        <label className="text-[11px] text-slate-500 mb-1.5 block">文件保存路径</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={saveDir}
            onChange={(e) => handleSaveDirChange(e.target.value)}
            className="flex-1 bg-white border border-slate-200 rounded-xl px-3 py-2.5 text-sm text-slate-800 focus:outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-50 transition-all"
          />
          <button
            onClick={handleSelectFolder}
            className="flex items-center justify-center w-10 h-10 rounded-xl bg-white border border-slate-200 hover:bg-slate-50 transition-colors"
          >
            <FolderOpen className="w-4 h-4 text-slate-400" />
          </button>
        </div>
      </div>

      {/* 开关项 */}
      <div className="bg-white border border-slate-200 rounded-xl overflow-hidden">
        {/* 同步开关 */}
        <button
          onClick={() => setSyncEnabled(!syncEnabled)}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 transition-colors border-b border-slate-100"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800">剪贴板同步</p>
            <p className="text-[11px] text-slate-400 mt-0.5">
              自动同步文本和图片到其他设备
            </p>
          </div>
          <Toggle checked={syncEnabled} onChange={() => setSyncEnabled(!syncEnabled)} />
        </button>

        {/* 开机自启 */}
        <button
          onClick={handleToggleAutoStart}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 transition-colors border-b border-slate-100"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800">开机自启</p>
            <p className="text-[11px] text-slate-400 mt-0.5">
              登录系统时自动启动 ShareCopy
            </p>
          </div>
          <Toggle checked={autoStart} onChange={handleToggleAutoStart} />
        </button>

        {/* 自动接收 */}
        <button
          onClick={handleToggleAutoAccept}
          className="w-full flex items-center justify-between p-3.5 hover:bg-slate-50 transition-colors"
        >
          <div className="text-left">
            <p className="text-sm font-medium text-slate-800">自动接收文件</p>
            <p className="text-[11px] text-slate-400 mt-0.5">
              无需确认直接保存接收的文件
            </p>
          </div>
          <Toggle checked={autoAccept} onChange={handleToggleAutoAccept} />
        </button>
      </div>

      {/* 同步统计 */}
      <div className="pt-4 border-t border-slate-200">
        <p className="text-[11px] text-slate-500 mb-2">同步统计</p>
        <StatsSection />
      </div>

      {/* 版本信息 */}
      <div className="pt-4 border-t border-slate-200">
        <p className="text-[11px] text-slate-400">
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
      <div className="p-3 rounded-xl bg-white border border-slate-200">
        <p className="text-lg font-bold text-slate-800">{stats.texts_synced}</p>
        <p className="text-[10px] text-slate-400 mt-0.5">文本</p>
      </div>
      <div className="p-3 rounded-xl bg-white border border-slate-200">
        <p className="text-lg font-bold text-slate-800">{stats.images_synced}</p>
        <p className="text-[10px] text-slate-400 mt-0.5">图片</p>
      </div>
      <div className="p-3 rounded-xl bg-white border border-slate-200">
        <p className="text-lg font-bold text-slate-800">{stats.files_transferred}</p>
        <p className="text-[10px] text-slate-400 mt-0.5">文件</p>
      </div>
    </div>
  );
}
