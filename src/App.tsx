import { useState } from 'react';
import { DeviceList } from './components/DeviceList';
import { Settings } from './components/Settings';
import { ClipboardHistory } from './components/ClipboardHistory';
import { Toaster } from 'react-hot-toast';

type Page = 'devices' | 'settings' | 'history';

function App() {
  const [activePage, setActivePage] = useState<Page>('devices');

  const navItems: { id: Page; label: string; icon: string }[] = [
    { id: 'devices', label: '设备', icon: '💻' },
    { id: 'history', label: '历史', icon: '📋' },
    { id: 'settings', label: '设置', icon: '⚙️' },
  ];

  return (
    <div className="flex flex-col h-screen bg-slate-950 text-slate-200">
      {/* 顶部标题栏 */}
      <header className="flex items-center justify-between px-4 py-3 border-b border-slate-800">
        <h1 className="text-lg font-semibold text-white">ShareCopy</h1>
        <span className="text-xs text-slate-500">v0.1.0</span>
      </header>

      {/* 主内容区 */}
      <main className="flex-1 overflow-y-auto p-4">
        {activePage === 'devices' && <DeviceList />}
        {activePage === 'settings' && <Settings />}
        {activePage === 'history' && <ClipboardHistory />}
      </main>

      {/* 底部导航 */}
      <nav className="flex border-t border-slate-800">
        {navItems.map((item) => (
          <button
            key={item.id}
            onClick={() => setActivePage(item.id)}
            className={`flex-1 flex flex-col items-center py-3 text-xs transition-colors ${
              activePage === item.id
                ? 'text-blue-400 bg-slate-900'
                : 'text-slate-500 hover:text-slate-300'
            }`}
          >
            <span className="text-lg mb-1">{item.icon}</span>
            {item.label}
          </button>
        ))}
      </nav>

      <Toaster
        position="top-center"
        toastOptions={{
          style: {
            background: '#1e293b',
            color: '#e2e8f0',
            fontSize: '14px',
          },
        }}
      />
    </div>
  );
}

export default App;
