import { useState, useEffect } from 'react';
import { Monitor, ClipboardList, Settings } from 'lucide-react';
import { DeviceList } from './components/DeviceList';
import { Settings as SettingsPage } from './components/Settings';
import { ClipboardHistory } from './components/ClipboardHistory';
import { TransferProgressPanel } from './components/TransferProgress';

type Page = 'devices' | 'settings' | 'history';
type Theme = 'system' | 'light' | 'dark';

const THEME_KEY = 'sharecopy-theme';

function getStoredTheme(): Theme {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  } catch { /* noop */ }
  return 'system';
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else if (theme === 'light') {
    root.classList.remove('dark');
  } else {
    // system
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    root.classList.toggle('dark', prefersDark);
  }
}

function App() {
  const [activePage, setActivePage] = useState<Page>('devices');
  const [theme, setThemeState] = useState<Theme>(getStoredTheme);

  const setTheme = (t: Theme) => {
    setThemeState(t);
    try { localStorage.setItem(THEME_KEY, t); } catch { /* noop */ }
    applyTheme(t);
  };

  // 初始化主题
  useEffect(() => {
    applyTheme(theme);

    // 跟随系统主题变化
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => {
      if (theme === 'system') applyTheme('system');
    };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [theme]);

  const navItems: { id: Page; label: string; icon: typeof Monitor }[] = [
    { id: 'devices', label: '设备', icon: Monitor },
    { id: 'history', label: '历史', icon: ClipboardList },
    { id: 'settings', label: '设置', icon: Settings },
  ];

  return (
    <div className="flex flex-col h-screen bg-slate-50 dark:bg-slate-950 text-slate-800 dark:text-slate-200 transition-colors">
      {/* 顶部标题栏 */}
      <header className="flex items-center justify-between px-4 py-3 bg-white dark:bg-slate-900 border-b border-slate-200 dark:border-slate-800 transition-colors">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-amber-500 to-orange-500 flex items-center justify-center">
            <span className="text-white font-bold text-xs">S</span>
          </div>
          <h1 className="text-base font-semibold text-slate-800 dark:text-slate-200">ShareCopy</h1>
        </div>
        <span className="text-[11px] text-slate-400 dark:text-slate-500">v0.1.0</span>
      </header>

      {/* 主内容区 */}
      <main className="flex-1 overflow-y-auto p-4">
        {activePage === 'devices' && <DeviceList />}
        {activePage === 'settings' && <SettingsPage theme={theme} setTheme={setTheme} />}
        {activePage === 'history' && <ClipboardHistory />}
      </main>

      {/* 传输进度 */}
      <TransferProgressPanel />

      {/* 底部导航 */}
      <nav className="flex bg-white dark:bg-slate-900 border-t border-slate-200 dark:border-slate-800 px-2 py-1 gap-1 transition-colors">
        {navItems.map((item) => {
          const isActive = activePage === item.id;
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              onClick={() => setActivePage(item.id)}
              className={`flex-1 flex flex-col items-center py-1.5 rounded-lg text-[11px] transition-colors ${
                isActive
                  ? 'text-amber-500 bg-amber-50 dark:bg-amber-500/10'
                  : 'text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300'
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
