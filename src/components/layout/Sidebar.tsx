import { NavLink } from 'react-router-dom';
import { useState, useEffect } from 'react';
import {
  Home,
  Clock,
  Settings,
  LogOut,
  Download,
  Loader2,
  HelpCircle,
} from 'lucide-react';
import { useAppStore } from '../../stores/app-store';
import { getCachedUpdate, setCachedUpdate } from '../../App';

import { useTranslation } from '../../locales';

const NAV_ITEMS = [
  { path: '/', icon: Home, labelKey: 'dashboard' },
  { path: '/workshop', icon: Clock, labelKey: 'workshop' },
  { path: '/settings', icon: Settings, labelKey: 'settings' },
];

export function Sidebar() {
  const status = useAppStore((s) => s.status);
  const availableUpdate = useAppStore((s) => s.availableUpdate);
  const { t } = useTranslation();
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';
  const [ver, setVer] = useState('');
  const [showUpdatePopup, setShowUpdatePopup] = useState(false);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(v => setVer(`v${v}`)).catch(() => {});
  }, []);

  const handleInstallUpdate = async () => {
    setInstalling(true);
    try {
      let update = getCachedUpdate();
      if (!update) {
        const { check } = await import('@tauri-apps/plugin-updater');
        update = await check();
      }
      if (update) {
        await update.downloadAndInstall();
        setCachedUpdate(null);
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch();
      }
    } catch (e) {
      console.error('Update failed:', e);
      setInstalling(false);
    }
  };

  return (
    <aside className="w-[92px] h-full bg-black flex flex-col items-center py-4 border-r-[4px] border-black/20 text-white">
      {/* Connection status indicator + update badge */}
      <div className="mb-4 relative shrink-0 w-12 h-4 flex items-center justify-center">
        {/* Green dot = connected, amber pulse = connecting */}
        {(isConnected || isConnecting) && (
          <div className={`w-3 h-3 rounded-full border-[2px] border-white/30
            ${isConnected ? 'bg-[#4ade80]' : 'bg-[#fbbf24] animate-pulse'}`} />
        )}
        {/* Update badge */}
        {availableUpdate && (
          <div className="absolute -top-1 -right-1 w-4 h-4 bg-red-500 rounded-full border-[2px] border-black animate-pulse cursor-pointer"
            onClick={() => setShowUpdatePopup(!showUpdatePopup)} />
        )}
        {/* Update popup */}
        {showUpdatePopup && availableUpdate && (
          <div className="absolute left-full ml-3 top-0 z-50 animate-slide-in">
            <div className="bg-white border-[3px] border-black rounded-2xl p-4 shadow-[6px_6px_0_#000] w-56 space-y-3">
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 bg-red-500 rounded-full animate-pulse" />
                <p className="text-[10px] font-black text-black uppercase tracking-widest">{t('newUpdate')}</p>
              </div>
              <p className="text-xs font-bold text-black/70">
                v{availableUpdate} {t('versionAvailable')}
              </p>
              <button
                onClick={handleInstallUpdate}
                disabled={installing}
                className="w-full py-2.5 bg-black text-white border-[3px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_#000] hover:-translate-y-0.5 hover:shadow-[5px_5px_0_#000] active:translate-y-1 active:shadow-none transition-all disabled:opacity-50 flex items-center justify-center gap-2"
              >
                {installing ? (
                  <><Loader2 className="w-3.5 h-3.5 animate-spin" /> {t('installingUpdate')}</>
                ) : (
                  <><Download className="w-3.5 h-3.5 stroke-[3px]" /> {t('installRestart')}</>
                )}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Navigation */}
      <nav className="flex flex-col gap-3 flex-1 w-full items-center">
        {NAV_ITEMS.map(({ path, icon: Icon, labelKey }) => (
          <NavLink
            key={path}
            to={path}
            className={({ isActive }) =>
              `group relative flex flex-col items-center justify-center w-16 h-16 rounded-2xl transition-all duration-150 overflow-hidden cursor-pointer gap-1
              ${
                isActive
                  ? 'bg-bg-primary border-[3px] border-black shadow-[4px_4px_0_rgba(255,255,255,0.2)]'
                  : 'bg-transparent text-white/50 hover:bg-white/10 hover:text-white border-[3px] border-transparent hover:border-white/20'
              }`
            }
          >
            {({ isActive }) => (
              <>
                <Icon className={`w-5 h-5 relative z-10 transition-transform stroke-[2.5px] ${isActive ? 'scale-110 text-black' : 'group-hover:scale-110'}`} />
                <span className={`text-[7px] font-black uppercase tracking-wider relative z-10 leading-none text-center whitespace-nowrap ${isActive ? 'text-black' : ''}`}>
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
            const { openUrl } = await import('@tauri-apps/plugin-opener');
            await openUrl('https://t.me/doodlevpn_support');
          } catch (e) {
            console.error(e);
          }
        }}
        className="group relative flex items-center justify-center w-16 h-16 rounded-2xl bg-black text-[#8b5cf6] hover:bg-[#8b5cf6] hover:text-black border-[3px] border-[#8b5cf6] hover:border-black shadow-[4px_4px_0_rgba(139,92,246,0.3)] hover:shadow-[4px_4px_0_#8b5cf6] transition-all duration-150 cursor-pointer mb-3"
        title="Support"
      >
        <HelpCircle className="w-6 h-6 transition-transform group-hover:scale-110 stroke-[2.5px]" />
        <span className="absolute left-full ml-4 px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white text-[#8b5cf6] border-[3px] border-[#8b5cf6] rounded-xl opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[4px_4px_0_#8b5cf6] z-50 translate-x-[-10px] group-hover:translate-x-0">
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
        className="group relative flex items-center justify-center w-16 h-16 rounded-2xl bg-black text-danger hover:bg-danger hover:text-black border-[3px] border-danger hover:border-black shadow-[4px_4px_0_rgba(248,113,113,0.3)] hover:shadow-[4px_4px_0_#f87171] transition-all duration-150 cursor-pointer mb-4"
        title="Quit DoodleRay"
      >
        <LogOut className="w-6 h-6 transition-transform group-hover:scale-110 stroke-[2.5px]" />
        <span className="absolute left-full ml-4 px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white text-danger border-[3px] border-danger rounded-xl opacity-0 group-hover:opacity-100 pointer-events-none transition-all duration-200 whitespace-nowrap shadow-[4px_4px_0_#f87171] z-50 translate-x-[-10px] group-hover:translate-x-0">
          {t('quit')}
        </span>
      </button>

      {ver && <div className="text-[10px] text-white/30 font-black tracking-widest px-2 py-1 rotate-[-90deg] origin-center -translate-y-8 absolute left-[-24px] bottom-16 opacity-50">{ver}</div>}
    </aside>
  );
}
