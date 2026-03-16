import { NavLink } from 'react-router-dom';
import { useState, useEffect } from 'react';
import {
  Zap,
  Home,
  Server,
  Wrench,
  Settings,
  LogOut,
} from 'lucide-react';
import { useAppStore } from '../../stores/app-store';

import { useTranslation } from '../../locales';

const NAV_ITEMS = [
  { path: '/', icon: Home, labelKey: 'dashboard' },
  { path: '/servers', icon: Server, labelKey: 'servers' },
  { path: '/workshop', icon: Wrench, labelKey: 'workshop' },
  { path: '/settings', icon: Settings, labelKey: 'settings' },
];

export function Sidebar() {
  const status = useAppStore((s) => s.status);
  const { t } = useTranslation();
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';
  const [ver, setVer] = useState('');

  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(v => setVer(`v${v}`)).catch(() => {});
  }, []);

  return (
    <aside className="w-[80px] h-full bg-black flex flex-col items-center py-6 border-r-[4px] border-black/20">
      {/* Logo + connection status indicator */}
      <div className="mb-8 relative shrink-0">
        <div className="flex items-center justify-center w-12 h-12 rounded-xl shadow-[4px_4px_0_rgba(255,255,255,0.2)] bg-bg-primary border-[3px] border-white">
          <Zap className="w-6 h-6 text-black stroke-[3px]" />
        </div>
        {/* Green dot = connected, amber pulse = connecting */}
        {(isConnected || isConnecting) && (
          <div className={`absolute -bottom-1 -right-1 w-4 h-4 rounded-full border-[3px] border-black
            ${isConnected ? 'bg-[#4ade80]' : 'bg-[#fbbf24] animate-pulse'}`} />
        )}
      </div>

      {/* Navigation */}
      <nav className="flex flex-col gap-4 flex-1 w-full px-3">
        {NAV_ITEMS.map(({ path, icon: Icon, labelKey }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              `group relative flex items-center justify-center w-full h-14 rounded-2xl transition-all duration-150 overflow-hidden cursor-pointer
              ${
                isActive
                  ? 'bg-bg-primary border-[3px] border-black shadow-[4px_4px_0_rgba(255,255,255,0.2)]'
                  : 'bg-transparent text-white/50 hover:bg-white/10 hover:text-white border-[3px] border-transparent hover:border-white/20'
              }`
            }
          >
            {({ isActive }) => (
              <>
                <Icon className={`w-6 h-6 relative z-10 transition-transform stroke-[2.5px] ${isActive ? 'scale-110 text-black' : 'group-hover:scale-110'}`} />
                <span className="absolute left-full ml-4 px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white text-black border-[3px] border-black rounded-xl opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[4px_4px_0_#000] z-50 translate-x-[-10px] group-hover:translate-x-0">
                  {t(labelKey as any)}
                </span>
                <div className={`absolute left-0 top-1/2 -translate-y-1/2 w-1.5 h-1/2 bg-black rounded-r-full transition-all duration-300 ${isActive ? 'opacity-100 scale-y-100' : 'opacity-0 scale-y-0'}`} />
              </>
            )}
          </NavLink>
        ))}
      </nav>
      <button
        onClick={async () => {
          try {
            const { invoke } = await import('@tauri-apps/api/core');
            await invoke('vpn_disconnect').catch(() => {});
            await invoke('quit_app');
          } catch {
            window.close();
          }
        }}
        className="group relative flex items-center justify-center w-14 h-14 rounded-2xl bg-black text-danger hover:bg-danger hover:text-black border-[3px] border-danger hover:border-black shadow-[4px_4px_0_rgba(248,113,113,0.3)] hover:shadow-[4px_4px_0_#f87171] transition-all duration-150 cursor-pointer mb-4"
        title="Quit DoodleRay"
      >
        <LogOut className="w-6 h-6 transition-transform group-hover:scale-110 stroke-[2.5px]" />
        <span className="absolute left-full ml-4 px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white text-danger border-[3px] border-danger rounded-xl opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[4px_4px_0_#f87171] z-50 translate-x-[-10px] group-hover:translate-x-0">
          Quit
        </span>
      </button>

      {ver && <div className="text-[10px] text-white/30 font-black tracking-widest px-2 py-1 rotate-[-90deg] origin-center -translate-y-8 absolute left-[-24px] bottom-16 opacity-50">{ver}</div>}
    </aside>
  );
}
