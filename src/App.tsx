import { useState } from 'react';
import { Monitor, ClipboardList, Settings } from 'lucide-react';
import { DeviceList } from './components/DeviceList';
import { Settings as SettingsPage } from './components/Settings';
import { ClipboardHistory } from './components/ClipboardHistory';
import { TransferProgressPanel } from './components/TransferProgress';

type Page = 'devices' | 'settings' | 'history';

function App() {
  const [activePage, setActivePage] = useState<Page>('devices');

  const navItems: { id: Page; label: string; icon: typeof Monitor }[] = [
    { id: 'devices', label: '设备', icon: Monitor },
    { id: 'history', label: '历史', icon: ClipboardList },
    { id: 'settings', label: '设置', icon: Settings },
  ];

  return (
    <div className="flex flex-col h-screen bg-slate-50 text-slate-800">
      {/* 顶部标题栏 */}
      <header className="flex items-center justify-between px-4 py-3 bg-white border-b border-slate-200">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-amber-500 to-orange-500 flex items-center justify-center">
            <span className="text-white font-bold text-xs">S</span>
          </div>
          <h1 className="text-base font-semibold text-slate-800">ShareCopy</h1>
        </div>
        <span className="text-[11px] text-slate-400">v0.1.0</span>
      </header>

      {/* 主内容区 */}
      <main className="flex-1 overflow-y-auto p-4">
        {activePage === 'devices' && <DeviceList />}
        {activePage === 'settings' && <SettingsPage />}
        {activePage === 'history' && <ClipboardHistory />}
      </main>

      {/* 传输进度 */}
      <TransferProgressPanel />

      {/* 底部导航 */}
      <nav className="flex bg-white border-t border-slate-200 px-2 py-1 gap-1">
        {navItems.map((item) => {
          const isActive = activePage === item.id;
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              onClick={() => setActivePage(item.id)}
              className={`flex-1 flex flex-col items-center py-1.5 rounded-lg text-[11px] transition-colors ${
                isActive
                  ? 'text-amber-500 bg-amber-50'
                  : 'text-slate-400 hover:text-slate-600'
              }`}
            >
              <Icon className={`w-[18px] h-[18px] mb-0.5 ${isActive ? 'stroke-amber-500' : ''}`} />
              {item.label}
            </button>
          );
        })}
      </nav>
    </div>
  );
}

export default App;
