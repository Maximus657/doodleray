import { NavLink } from 'react-router-dom';
import { useState, useEffect } from 'react';
import {
  Home,
  Server,
  Clock,
  Settings,
  LogOut,
  HelpCircle,
} from 'lucide-react';

import { useTranslation } from '../../locales';

const NAV_ITEMS = [
  { path: '/', icon: Home, labelKey: 'dashboard' },
  { path: '/servers', icon: Server, labelKey: 'servers' },
  { path: '/workshop', icon: Clock, labelKey: 'workshop' },
  { path: '/settings', icon: Settings, labelKey: 'settings' },
];

export function Sidebar() {
  const { t } = useTranslation();
  const [ver, setVer] = useState('');

  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(v => setVer(`v${v}`)).catch(() => {});
  }, []);

  return (
    <aside className="relative w-[86px] h-full bg-black flex flex-col items-center py-5 border-r-[4px] border-black/20 text-white">

      {/* Navigation */}
      <nav className="flex flex-col gap-3 flex-1 w-full items-center pt-1">
        {NAV_ITEMS.map(({ path, icon: Icon, labelKey }) => (
          <NavLink
            key={path}
            to={path}
            aria-label={t(labelKey as any)}
            title={t(labelKey as any)}
            className={({ isActive }) =>
              `group relative flex h-14 w-14 items-center justify-center rounded-2xl border-[2px] transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] cursor-pointer
              ${
                isActive
                  ? 'bg-bg-primary text-black border-black shadow-[4px_4px_0_rgba(255,255,255,0.16)] -translate-x-0.5'
                  : 'bg-white/[0.04] text-white/45 border-white/10 hover:bg-white/10 hover:text-white hover:border-white/25'
              }`
            }
          >
            {({ isActive }) => (
              <>
                <Icon className={`w-6 h-6 relative z-10 transition-transform duration-300 stroke-[2.6px] ${isActive ? 'scale-110 text-black' : 'group-hover:scale-110'}`} />
                <span className="pointer-events-none absolute left-full ml-3 rounded-lg border-[2px] border-black bg-white px-2.5 py-1.5 text-[10px] font-black uppercase tracking-widest text-black opacity-0 shadow-[3px_3px_0_rgba(0,0,0,0.45)] transition-all duration-200 translate-x-[-8px] group-hover:translate-x-0 group-hover:opacity-100 whitespace-nowrap z-50">
                  {t(labelKey as any)}
                </span>
                <div className={`absolute -right-1 top-1/2 h-6 w-1.5 -translate-y-1/2 rounded-l-full bg-bg-primary transition-all duration-300 ${isActive ? 'opacity-100 scale-y-100' : 'opacity-0 scale-y-50'}`} />
              </>
            )}
          </NavLink>
        ))}
      </nav>

      <button
        onClick={async () => {
          try {
            const { openUrl } = await import('@tauri-apps/plugin-opener');
            await openUrl('https://t.me/doodlevpn_support');
          } catch (e) {
            console.error(e);
          }
        }}
        className="group relative flex h-14 w-14 items-center justify-center rounded-2xl border-[2px] border-[#8b5cf6]/70 bg-white/[0.04] text-[#8b5cf6] transition-all duration-300 hover:-translate-y-0.5 hover:border-[#8b5cf6] hover:bg-[#8b5cf6] hover:text-black hover:shadow-[4px_4px_0_rgba(139,92,246,0.35)] active:translate-y-0 active:shadow-none cursor-pointer mb-3"
        title="Support"
      >
        <HelpCircle className="w-6 h-6 transition-transform duration-300 group-hover:scale-110 stroke-[2.6px]" />
        <span className="absolute left-full ml-3 px-2.5 py-1.5 text-[10px] font-black uppercase tracking-widest bg-white text-[#8b5cf6] border-[2px] border-[#8b5cf6] rounded-lg opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[3px_3px_0_rgba(139,92,246,0.35)] z-50 translate-x-[-8px] group-hover:translate-x-0">
          {t('support' as any)}
        </span>
      </button>

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
        className="group relative flex h-14 w-14 items-center justify-center rounded-2xl border-[2px] border-danger/80 bg-white/[0.04] text-danger transition-all duration-300 hover:-translate-y-0.5 hover:border-danger hover:bg-danger hover:text-black hover:shadow-[4px_4px_0_rgba(248,113,113,0.32)] active:translate-y-0 active:shadow-none cursor-pointer mb-4"
        title="Quit DoodleRay"
      >
        <LogOut className="w-6 h-6 transition-transform duration-300 group-hover:scale-110 stroke-[2.6px]" />
        <span className="absolute left-full ml-3 px-2.5 py-1.5 text-[10px] font-black uppercase tracking-widest bg-white text-danger border-[2px] border-danger rounded-lg opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[3px_3px_0_rgba(248,113,113,0.35)] z-50 translate-x-[-8px] group-hover:translate-x-0">
          {t('quit')}
        </span>
      </button>

      {ver && <div className="absolute bottom-2 left-1/2 -translate-x-1/2 text-[9px] text-white/22 font-black tracking-widest opacity-60">{ver}</div>}
    </aside>
  );
}
