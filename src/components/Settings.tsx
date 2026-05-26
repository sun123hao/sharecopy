import { useEffect, useState } from 'react';
import { FolderOpen, ToggleLeft, ToggleRight } from 'lucide-react';
import { useAppStore } from '../store/useAppStore';

export function Settings() {
  const config = useAppStore((s) => s.config);
  const syncEnabled = useAppStore((s) => s.syncEnabled);
  const loadConfig = useAppStore((s) => s.loadConfig);
  const setSyncEnabled = useAppStore((s) => s.setSyncEnabled);
  const updateDeviceName = useAppStore((s) => s.updateDeviceName);
  const updateConfigAction = useAppStore((s) => s.updateConfig);

  // 本地编辑状态
  const [deviceName, setDeviceName] = useState('');
  const [saveDir, setSaveDir] = useState('');
  const [autoStart, setAutoStart] = useState(false);
  const [autoAccept, setAutoAccept] = useState(true);

  // 从 store 同步到本地状态
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
      const updated = { ...config, save_dir: dir };
      updateConfigAction(updated);
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

  const handleToggleSync = () => {
    setSyncEnabled(!syncEnabled);
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
      <h2 className="text-sm font-medium text-slate-400 mb-4">设置</h2>

      {/* 设备别名 */}
      <div>
        <label className="text-xs text-slate-500 mb-1.5 block">设备名称</label>
        <input
          type="text"
          value={deviceName}
          onChange={(e) => setDeviceName(e.target.value)}
          onBlur={handleDeviceNameBlur}
          className="w-full bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-blue-500 transition-colors"
        />
      </div>

      {/* 文件保存路径 */}
      <div>
        <label className="text-xs text-slate-500 mb-1.5 block">
          文件保存路径
        </label>
        <div className="flex gap-2">
          <input
            type="text"
            value={saveDir}
            onChange={(e) => handleSaveDirChange(e.target.value)}
            className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-blue-500 transition-colors"
          />
          <button
            onClick={handleSelectFolder}
            className="flex items-center justify-center w-9 h-9 rounded-lg bg-slate-800 hover:bg-slate-700 transition-colors"
          >
            <FolderOpen className="w-4 h-4 text-slate-400" />
          </button>
        </div>
      </div>

      {/* 开关项 */}
      <div className="space-y-3 pt-2">
        {/* 同步开关 */}
        <button
          onClick={handleToggleSync}
          className="w-full flex items-center justify-between p-3 rounded-lg bg-slate-900 border border-slate-800"
        >
          <div className="text-left">
            <p className="text-sm text-white">剪贴板同步</p>
            <p className="text-xs text-slate-500 mt-0.5">
              自动同步文本和图片到其他设备
            </p>
          </div>
          {syncEnabled ? (
            <ToggleRight className="w-6 h-6 text-blue-400" />
          ) : (
            <ToggleLeft className="w-6 h-6 text-slate-600" />
          )}
        </button>

        {/* 开机自启 */}
        <button
          onClick={handleToggleAutoStart}
          className="w-full flex items-center justify-between p-3 rounded-lg bg-slate-900 border border-slate-800"
        >
          <div className="text-left">
            <p className="text-sm text-white">开机自启</p>
            <p className="text-xs text-slate-500 mt-0.5">
              登录系统时自动启动 ShareCopy
            </p>
          </div>
          {autoStart ? (
            <ToggleRight className="w-6 h-6 text-blue-400" />
          ) : (
            <ToggleLeft className="w-6 h-6 text-slate-600" />
          )}
        </button>

        {/* 自动接收 */}
        <button
          onClick={handleToggleAutoAccept}
          className="w-full flex items-center justify-between p-3 rounded-lg bg-slate-900 border border-slate-800"
        >
          <div className="text-left">
            <p className="text-sm text-white">自动接收文件</p>
            <p className="text-xs text-slate-500 mt-0.5">
              无需确认直接保存接收的文件
            </p>
          </div>
          {autoAccept ? (
            <ToggleRight className="w-6 h-6 text-blue-400" />
          ) : (
            <ToggleLeft className="w-6 h-6 text-slate-600" />
          )}
        </button>
      </div>

      {/* 同步统计 */}
      <div className="pt-4 border-t border-slate-800">
        <p className="text-xs text-slate-500 mb-2">同步统计</p>
        <StatsSection />
      </div>

      {/* 版本信息 */}
      <div className="pt-4 border-t border-slate-800">
        <p className="text-xs text-slate-600">
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
      <div className="p-2 rounded bg-slate-900">
        <p className="text-lg font-semibold text-white">{stats.texts_synced}</p>
        <p className="text-xs text-slate-500">文本</p>
      </div>
      <div className="p-2 rounded bg-slate-900">
        <p className="text-lg font-semibold text-white">{stats.images_synced}</p>
        <p className="text-xs text-slate-500">图片</p>
      </div>
      <div className="p-2 rounded bg-slate-900">
        <p className="text-lg font-semibold text-white">{stats.files_transferred}</p>
        <p className="text-xs text-slate-500">文件</p>
      </div>
    </div>
  );
}
