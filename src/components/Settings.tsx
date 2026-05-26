import { useState } from 'react';
import { FolderOpen, ToggleLeft, ToggleRight } from 'lucide-react';

export function Settings() {
  const [deviceName, setDeviceName] = useState('My MacBook Pro');
  const [saveDir, setSaveDir] = useState('~/Downloads/ShareCopy');
  const [autoStart, setAutoStart] = useState(false);
  const [syncEnabled, setSyncEnabled] = useState(true);
  const [autoAccept, setAutoAccept] = useState(true);

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
            onChange={(e) => setSaveDir(e.target.value)}
            className="flex-1 bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-blue-500 transition-colors"
          />
          <button className="flex items-center justify-center w-9 h-9 rounded-lg bg-slate-800 hover:bg-slate-700 transition-colors">
            <FolderOpen className="w-4 h-4 text-slate-400" />
          </button>
        </div>
      </div>

      {/* 开关项 */}
      <div className="space-y-3 pt-2">
        {/* 同步开关 */}
        <button
          onClick={() => setSyncEnabled(!syncEnabled)}
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
          onClick={() => setAutoStart(!autoStart)}
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
          onClick={() => setAutoAccept(!autoAccept)}
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

      {/* 版本信息 */}
      <div className="pt-4 border-t border-slate-800">
        <p className="text-xs text-slate-600">
          ShareCopy v0.1.0 · Tauri + Rust + React
        </p>
      </div>
    </div>
  );
}
