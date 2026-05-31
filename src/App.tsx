import { useState, useEffect } from 'react';
import { Monitor, ClipboardList, Settings } from 'lucide-react';
import { DeviceList } from './components/DeviceList';
import { Settings as SettingsPage } from './components/Settings';
import { ClipboardHistory } from './components/ClipboardHistory';
import { TransferProgressPanel } from './components/TransferProgress';
import { DeviceTransferPage } from './components/DeviceTransferPage';
import { useAppStore } from './store/useAppStore';

type Page = 'devices' | 'settings' | 'history' | 'transfers';
type Theme = 'system' | 'light' | 'dark';

// 注意：此 key 和 applyTheme 逻辑与 index.html 内联脚本重复，
// 修改时必须两处同步更新，否则会导致初始主题闪烁或不同步。
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
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    root.classList.toggle('dark', prefersDark);
  }
}

function App() {
  const [activePage, setActivePage] = useState<Page>('devices');
  const [theme, setThemeState] = useState<Theme>(getStoredTheme);
  const selectDevice = useAppStore((s) => s.selectDevice);

  const setTheme = (t: Theme) => {
    setThemeState(t);
    try { localStorage.setItem(THEME_KEY, t); } catch { /* noop */ }
    applyTheme(t);
  };

  useEffect(() => {
    applyTheme(getStoredTheme());
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = () => { if (getStoredTheme() === 'system') applyTheme('system'); };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  const navItems: { id: Page; label: string; icon: typeof Monitor }[] = [
    { id: 'devices', label: '设备', icon: Monitor },
    { id: 'history', label: '历史', icon: ClipboardList },
    { id: 'settings', label: '设置', icon: Settings },
  ];

  return (
    <div
      className="flex flex-col h-screen bg-ios-bg dark:bg-black text-black dark:text-white transition-colors"
      style={{ paddingTop: 'var(--safe-area-top)', paddingBottom: 'var(--safe-area-bottom)' }}
    >
      {/* 顶部导航栏 — iOS 毛玻璃 */}
      <header className="flex items-center justify-between px-4 py-3 chrome border-b border-ios-separator dark:border-ios-separator-dark transition-colors flex-shrink-0">
        <div className="flex items-center gap-2">
          <div className="w-[30px] h-[30px] rounded-lg bg-gradient-to-br from-accent to-[#5856D6] flex items-center justify-center shadow-[0_2px_8px_rgba(0,122,255,0.35)]">
            <span className="text-white font-bold text-[13px]">S</span>
          </div>
          <h1 className="text-[17px] font-semibold">ShareCopy</h1>
        </div>
      </header>

      {/* 主内容区 */}
      <main className="flex-1 overflow-y-auto p-4">
        {activePage === 'devices' && (
          <DeviceList
            onSelectDevice={(deviceId) => { selectDevice(deviceId); setActivePage('transfers'); }}
          />
        )}
        {activePage === 'settings' && <SettingsPage theme={theme} setTheme={setTheme} />}
        {activePage === 'history' && <ClipboardHistory />}
        {activePage === 'transfers' && (
          <DeviceTransferPage onBack={() => { setActivePage('devices'); selectDevice(null); }} />
        )}
      </main>

      {/* 全局传输进度 — 仅在非设备详情页显示 */}
      {activePage !== 'transfers' && <TransferProgressPanel />}

      {/* 底部导航栏 — iOS 毛玻璃 Tab Bar */}
      <nav className="flex chrome border-t border-ios-separator dark:border-ios-separator-dark px-2 pt-1.5 pb-[22px] gap-1 transition-colors flex-shrink-0">
        {navItems.map((item) => {
          const isActive = activePage === item.id;
          const Icon = item.icon;
          return (
            <button
              key={item.id}
              onClick={() => { setActivePage(item.id); selectDevice(null); }}
              className={`flex-1 flex flex-col items-center py-1 rounded-[10px] text-[10px] font-medium transition-all duration-300 ${
                isActive
                  ? 'text-accent'
                  : 'text-black/30 dark:text-white/30'
              }`}
            >
              <Icon className={`w-[24px] h-[24px] mb-0.5 ${isActive ? '' : ''}`} />
              {item.label}
            </button>
          );
        })}
      </nav>
    </div>
  );
}

export default App;
