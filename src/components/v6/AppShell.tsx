import { useState, type ReactNode } from 'react';
import { HelpCircle, SlidersHorizontal } from 'lucide-react';
import { useAppStore } from '../../stores/app-store';
import { useTranslation } from '../../locales';
import { getSubscriptionById, getSubscriptionTrafficStatus } from '../../lib/subscription-status';
import WindowControls from './TitleBar';
import SupportModal from './SupportModal';
import SettingsModal from './SettingsModal';

/**
 * v6 shell, ported from the DoodleVPN Claude Design prototype: warm plum
 * glass panel and a design header (logo, traffic chip, support/settings,
 * Windows window controls on the undecorated window). The prototype's large
 * connected-state wallpaper blobs are intentionally removed: in the real app
 * they looked like full-window warning lights behind the content.
 */
export default function AppShell({ children }: { children: ReactNode }) {
  const subscriptions = useAppStore((s) => s.subscriptions);
  const activeServer = useAppStore((s) => s.activeServer);
  const serversCount = useAppStore((s) => s.servers.length);
  const status = useAppStore((s) => s.status);
  const appSessionLoggedIn = useAppStore((s) => s.appSessionLoggedIn);
  const { t } = useTranslation();
  const [supportOpen, setSupportOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const hasMainContent = appSessionLoggedIn || serversCount > 0 || status !== 'disconnected';

  // Traffic chip: real quota of the active subscription (design: "X.X GB left").
  const activeSub = getSubscriptionById(subscriptions, activeServer?.subscriptionId) ?? subscriptions[0] ?? null;
  const traffic = activeSub ? getSubscriptionTrafficStatus(activeSub) : null;
  const gbLeft = traffic?.hasQuota ? traffic.remaining / 1024 ** 3 : null;
  const pct = traffic?.hasQuota ? traffic.usedPercent / 100 : 0;
  const chipColor = pct > 0.85 ? '#ff6b5a' : pct > 0.7 ? '#ffb02e' : '#F97F16';

  const exportSupportBundle = async () => {
    const s = useAppStore.getState();
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const path = await invoke('export_support_bundle', {
        proxyMode: s.proxyMode,
        systemProxyMode: s.systemProxyMode,
        socksPort: s.socksPort,
        httpPort: s.httpPort,
      }) as string;
      s.addLog('success', `${t('supportBundleExported' as never)}: ${path}`);
      const { useToastStore } = await import('../../stores/toast-store');
      useToastStore.getState().addToast(t('supportBundleExported' as never), 'success');
    } catch (err) {
      s.addLog('error', `${t('supportBundleExportFailed' as never)}: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  return (
    <div className="v6-app relative flex h-screen w-screen flex-col overflow-hidden">
      <div className="v6-panel relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[34px] p-[18px]">
        {/* Top drag strip: covers the whole header band (incl. panel padding)
            so the window drags from anywhere up top except the buttons. */}
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-[5] h-[68px]" />

        {/* HEADER */}
        <div data-tauri-drag-region className="relative z-10 flex shrink-0 select-none items-center justify-between px-2.5 pb-4 pt-1.5">
          <div
            data-tauri-drag-region
            className={`pointer-events-none flex items-center gap-[11px] ${hasMainContent ? 'v6-brand-enter' : 'v6-brand-hidden'}`}
          >
            <img
              src="/assets/mascot.png"
              alt=""
              draggable={false}
              data-v6-brand-logo
              className="v6-brand-logo h-[34px] w-[34px] rounded-[11px]"
              style={{ boxShadow: '0 6px 18px rgba(234,109,6,0.45)' }}
            />
            <div className="v6-brand-word text-[19px] font-semibold tracking-[-0.01em] text-white">
              Doodle<span className="font-light text-white/70">Ray</span>
            </div>
          </div>

          <div className="flex items-center gap-2.5">
            {gbLeft !== null && (
              <div className="flex items-center gap-[9px] rounded-[30px] border border-white/[0.12] bg-white/[0.08] px-4 py-2">
                <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: chipColor, boxShadow: `0 0 8px ${chipColor}` }} />
                <span className="text-[13px] font-semibold text-white/90">
                  {gbLeft >= 100 ? Math.round(gbLeft) : gbLeft.toFixed(1)} <span className="font-normal text-white/50">{t('v6GbLeft' as never)}</span>
                </span>
              </div>
            )}
            <HeaderButton label={t('v6SupportTitle' as never)} onClick={() => setSupportOpen(true)}>
              <HelpCircle className="h-5 w-5" strokeWidth={2} />
            </HeaderButton>
            <HeaderButton label={t('settings' as never)} onClick={() => setSettingsOpen(true)}>
              <SlidersHorizontal className="h-5 w-5" strokeWidth={2} />
            </HeaderButton>
            <WindowControls />
          </div>
        </div>

        {/* CONTENT */}
        <main className="relative z-10 flex min-h-0 flex-1 flex-col">{children}</main>

        {/* OVERLAYS */}
        {supportOpen && (
          <SupportModal onClose={() => setSupportOpen(false)} onExportSupportBundle={exportSupportBundle} t={t} />
        )}
        {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} t={t} />}
      </div>
    </div>
  );
}

function HeaderButton({ children, onClick, label }: { children: ReactNode; onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="v6-hover-bright flex h-10 w-10 items-center justify-center rounded-[13px] border border-white/[0.12] bg-white/[0.07] text-white/[0.78] v6-focus"
    >
      {children}
    </button>
  );
}
