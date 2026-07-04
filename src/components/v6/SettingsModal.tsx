import { X, SlidersHorizontal, ChevronRight, LogOut } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAppStore, type SupportedLanguage } from '../../stores/app-store';
import Toggle from './Toggle';

type T = (key: never) => string;

/**
 * Design "Settings" overlay wired to real store flags. Advanced settings
 * (ports, DNS, repair, updates) stay on the full Settings page.
 */
export default function SettingsModal({ onClose, t }: { onClose: () => void; t: T }) {
  const navigate = useNavigate();
  const {
    autoStart, silentAdminAutostart,
    autoConnectOnStartup, setAutoConnectOnStartup,
    killSwitch, setKillSwitch,
    showStats, setShowStats,
    language, setLanguage,
  } = useAppStore();

  const launchOn = autoStart || silentAdminAutostart;

  const toggleLaunch = async () => {
    if (silentAdminAutostart) { navigate('/settings'); onClose(); return; }
    const next = !autoStart;
    useAppStore.setState({ autoStart: next });
    try {
      const { enable, disable } = await import('@tauri-apps/plugin-autostart');
      if (next) await enable(); else await disable();
    } catch {
      useAppStore.setState({ autoStart: !next });
    }
  };

  const rows: Array<{ title: string; sub: string; on: boolean; onClick: () => void }> = [
    { title: t('v6SetLaunch' as never), sub: t('v6SetLaunchSub' as never), on: launchOn, onClick: toggleLaunch },
    { title: t('v6SetAutoConnect' as never), sub: t('v6SetAutoConnectSub' as never), on: autoConnectOnStartup, onClick: () => setAutoConnectOnStartup(!autoConnectOnStartup) },
    { title: t('v6SetKillSwitch' as never), sub: t('v6SetKillSwitchSub' as never), on: killSwitch, onClick: () => setKillSwitch(!killSwitch) },
    { title: t('v6SetStats' as never), sub: t('v6SetStatsSub' as never), on: showStats, onClick: () => setShowStats(!showStats) },
  ];

  return (
    <div
      onClick={onClose}
      className="v6-fadein absolute inset-0 z-20 flex items-center justify-center"
      style={{ background: 'rgba(10,5,8,0.5)', backdropFilter: 'blur(8px)', WebkitBackdropFilter: 'blur(8px)' }}
    >
      <div onClick={(e) => e.stopPropagation()} className="v6-modal w-[min(440px,calc(100vw-48px))] rounded-[28px] p-[26px]">
        <div className="mb-[18px] flex items-center justify-between">
          <span className="text-[18px] font-semibold text-white">{t('settings' as never)}</span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('cancel' as never)}
            className="v6-hover-bright flex h-[34px] w-[34px] items-center justify-center rounded-[11px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
          >
            <X className="h-4 w-4" strokeWidth={2.3} />
          </button>
        </div>

        <div className="flex flex-col">
          {rows.map(({ title, sub, on, onClick }) => (
            <button
              key={title}
              type="button"
              onClick={onClick}
              className="flex w-full items-center gap-3 border-b border-white/[0.07] px-1 py-3.5 text-left v6-focus"
            >
              <span className="min-w-0 flex-1">
                <span className="block text-[14.5px] font-medium text-white">{title}</span>
                <span className="mt-0.5 block text-[11.5px] text-white/45">{sub}</span>
              </span>
              <Toggle on={on} label={title} />
            </button>
          ))}

          {/* Language */}
          <div className="flex items-center gap-3 border-b border-white/[0.07] px-1 py-3.5">
            <span className="min-w-0 flex-1">
              <span className="block text-[14.5px] font-medium text-white">{t('v6SetLanguage' as never)}</span>
              <span className="mt-0.5 block text-[11.5px] text-white/45">{t('v6SetLanguageSub' as never)}</span>
            </span>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value as SupportedLanguage)}
              className="cursor-pointer rounded-xl border border-white/[0.14] bg-white/[0.08] px-3.5 py-2 text-[13.5px] font-medium text-white outline-none v6-focus [&>option]:bg-[#1c1116]"
            >
              <option value="en">English</option>
              <option value="ru">Русский</option>
              <option value="zh">中文</option>
            </select>
          </div>

          {/* All settings */}
          <button
            type="button"
            onClick={() => { navigate('/settings'); onClose(); }}
            className="flex w-full items-center gap-3 border-b border-white/[0.07] px-1 py-3.5 text-left v6-focus"
          >
            <span className="v6-tile-accent flex h-9 w-9 shrink-0 items-center justify-center rounded-xl">
              <SlidersHorizontal className="h-[17px] w-[17px]" strokeWidth={1.9} />
            </span>
            <span className="flex-1 text-[14.5px] font-medium text-white">{t('v6SetAll' as never)}</span>
            <ChevronRight className="h-4 w-4 text-white/40" strokeWidth={2.2} />
          </button>

          {/* Quit */}
          <button
            type="button"
            onClick={async () => {
              try {
                const { invoke } = await import('@tauri-apps/api/core');
                await invoke('vpn_disconnect').catch(() => {});
                await invoke('quit_app');
              } catch {
                window.close();
              }
            }}
            className="flex w-full items-center gap-3 px-1 py-3.5 text-left v6-focus"
          >
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-[#ff6b5a]/30 bg-[#ff6b5a]/15 text-[#ff8a7a]">
              <LogOut className="h-[17px] w-[17px]" strokeWidth={1.9} />
            </span>
            <span className="flex-1 text-[14.5px] font-medium text-[#ff9a8c]">{t('quit' as never)}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
