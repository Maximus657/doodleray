import { useEffect, useRef, useState, type ReactNode } from 'react';
import gsap from 'gsap';
import { useGSAP } from '@gsap/react';
import { HelpCircle, RectangleHorizontal, RectangleVertical, SlidersHorizontal } from 'lucide-react';
import { useAppStore } from '../../stores/app-store';
import { useToastStore } from '../../stores/toast-store';
import { useTranslation } from '../../locales';
import { getSubscriptionById, getSubscriptionTrafficStatus } from '../../lib/subscription-status';
import { isNetworkExtensionOnlyBuild } from '../../lib/build-policy';
import WindowControls from './TitleBar';
import SupportModal from './SupportModal';
import SettingsModal from './SettingsModal';
import { desktopBridge } from '../../platform/tauri/desktop-bridge';

gsap.registerPlugin(useGSAP);

type WindowMode = 'wide' | 'compact';

async function resizeNativeWindow(mode: WindowMode): Promise<void> {
  if (typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ === 'undefined') return;

  const [{ getCurrentWindow }, { LogicalSize }] = await Promise.all([
    import('@tauri-apps/api/window'),
    import('@tauri-apps/api/dpi'),
  ]);
  const appWindow = getCurrentWindow();
  const compact = mode === 'compact';
  const minSize = new LogicalSize(compact ? 420 : 940, compact ? 680 : 660);
  const targetSize = new LogicalSize(compact ? 440 : 1204, compact ? 760 : 764);

  // macOS ignores explicit frame changes while the window is in its zoomed
  // (Tauri "maximized") state. Restore it before applying compact dimensions.
  if (await appWindow.isMaximized()) await appWindow.unmaximize();

  if (compact) {
    await appWindow.setMinSize(minSize);
    await appWindow.setSize(targetSize);
  } else {
    await appWindow.setSize(targetSize);
    await appWindow.setMinSize(minSize);
  }

  const scaleFactor = await appWindow.scaleFactor();
  let actualSize = (await appWindow.innerSize()).toLogical(scaleFactor);
  const matchesTarget = () => (
    Math.abs(actualSize.width - targetSize.width) <= 8
    && Math.abs(actualSize.height - targetSize.height) <= 8
  );

  // A zoom transition can finish just after unmaximize() resolves. One retry
  // makes the switch deterministic without leaving compact CSS in a wide frame.
  if (!matchesTarget()) {
    if (await appWindow.isMaximized()) await appWindow.unmaximize();
    if (compact) await appWindow.setMinSize(minSize);
    await appWindow.setSize(targetSize);
    if (!compact) await appWindow.setMinSize(minSize);
    actualSize = (await appWindow.innerSize()).toLogical(scaleFactor);
  }

  if (!matchesTarget()) {
    throw new Error(`Native window remained ${Math.round(actualSize.width)}×${Math.round(actualSize.height)}`);
  }
}

function afterNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    let settled = false;
    const done = () => {
      if (settled) return;
      settled = true;
      window.clearTimeout(fallback);
      resolve();
    };
    const fallback = window.setTimeout(done, 250);
    requestAnimationFrame(() => requestAnimationFrame(done));
  });
}

/**
 * v6 shell, ported from the DoodleVPN Claude Design prototype: warm plum
 * glass panel and a design header (logo, traffic chip, support/settings,
 * Windows window controls on the undecorated window). The prototype's large
 * connected-state wallpaper blobs are intentionally removed: in the real app
 * they looked like full-window warning lights behind the content.
 */
export default function AppShell({ children }: { children: ReactNode }) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLElement | null>(null);
  const transitionRunningRef = useRef(false);
  const mountedRef = useRef(true);
  const subscriptions = useAppStore((s) => s.subscriptions);
  const activeServer = useAppStore((s) => s.activeServer);
  const serversCount = useAppStore((s) => s.servers.length);
  const status = useAppStore((s) => s.status);
  const appSessionLoggedIn = useAppStore((s) => s.appSessionLoggedIn);
  const { t } = useTranslation();
  const [supportOpen, setSupportOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [windowMode, setWindowMode] = useState<WindowMode>(() => {
    try { return localStorage.getItem('doodleray_window_mode') === 'compact' ? 'compact' : 'wide'; }
    catch { return 'wide'; }
  });
  const initialWindowModeRef = useRef(windowMode);
  const [windowTransitioning, setWindowTransitioning] = useState(false);
  const nativeMacWindow = isNetworkExtensionOnlyBuild();
  const hasMainContent = appSessionLoggedIn || serversCount > 0 || status !== 'disconnected';
  // Native App Store windows reserve the top-left corner for macOS traffic
  // lights even in compact mode, so their brand stays centered.
  const brandOnLeft = !nativeMacWindow;

  useEffect(() => {
    try { localStorage.setItem('doodleray_window_mode', windowMode); } catch { /* non-critical preference */ }
  }, [windowMode]);

  useEffect(() => {
    mountedRef.current = true;
    void resizeNativeWindow(initialWindowModeRef.current)
      .catch((error) => {
        useAppStore.getState().addLog('warning', `Window mode restore failed: ${error instanceof Error ? error.message : String(error)}`);
        if (initialWindowModeRef.current === 'compact') {
          setWindowMode('wide');
        }
      });
    return () => { mountedRef.current = false; };
  }, []);

  const { contextSafe } = useGSAP({ scope: rootRef });
  const toggleWindowMode = contextSafe(async () => {
    if (transitionRunningRef.current) return;
    transitionRunningRef.current = true;
    setWindowTransitioning(true);

    const content = contentRef.current;
    const nextMode: WindowMode = windowMode === 'compact' ? 'wide' : 'compact';
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const direction = nextMode === 'compact' ? -1 : 1;

    try {
      if (content && !reducedMotion) {
        await gsap.to(content, {
          autoAlpha: 0,
          x: direction * 14,
          scale: 0.985,
          duration: 0.16,
          ease: 'power2.in',
        });
      }

      await resizeNativeWindow(nextMode);
      setWindowMode(nextMode);
      await afterNextPaint();
      if (!mountedRef.current) return;

      if (content && !reducedMotion) {
        await gsap.fromTo(content, {
          autoAlpha: 0,
          x: direction * -18,
          scale: 0.985,
        }, {
          autoAlpha: 1,
          x: 0,
          scale: 1,
          duration: 0.38,
          ease: 'expo.out',
          clearProps: 'opacity,visibility,transform',
        });
      }
    } catch (error) {
      useAppStore.getState().addLog('error', `Window mode switch failed: ${error instanceof Error ? error.message : String(error)}`);
    } finally {
      if (content) gsap.set(content, { clearProps: 'opacity,visibility,transform' });
      transitionRunningRef.current = false;
      if (mountedRef.current) setWindowTransitioning(false);
    }
  });

  // Traffic chip: real quota of the active subscription (design: "X.X GB left").
  const activeSub = getSubscriptionById(subscriptions, activeServer?.subscriptionId) ?? subscriptions[0] ?? null;
  const traffic = activeSub ? getSubscriptionTrafficStatus(activeSub) : null;
  const gbLeft = traffic?.hasQuota ? traffic.remaining / 1024 ** 3 : null;
  const pct = traffic?.hasQuota ? traffic.usedPercent / 100 : 0;
  const chipColor = pct > 0.85 ? '#ff6b5a' : pct > 0.7 ? '#ffb02e' : '#F97F16';

  const exportSupportBundle = async () => {
    const s = useAppStore.getState();
    try {
      await desktopBridge.command(
        nativeMacWindow ? 'export_app_store_support_bundle' : 'export_support_bundle',
        nativeMacWindow
          ? {}
          : {
            proxyMode: s.proxyMode,
            systemProxyMode: s.systemProxyMode,
            socksPort: s.socksPort,
            httpPort: s.httpPort,
          },
      );
      s.addLog('success', t('supportBundleExported' as never));
      useToastStore.getState().addToast(t('supportBundleExported' as never), 'success');
    } catch (err) {
      s.addLog('error', `${t('supportBundleExportFailed' as never)}: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  return (
    <div ref={rootRef} className={`v6-app relative flex h-screen w-screen flex-col overflow-hidden${nativeMacWindow ? ' v6-native-mac-window' : ''}${windowMode === 'compact' ? ' v6-compact-mode' : ''}`} data-window-mode={windowMode}>
      <div className="v6-panel relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[34px] p-[18px]">
        {/* Top drag strip: covers the whole header band (incl. panel padding)
            so the window drags from anywhere up top except the buttons. */}
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 z-[5] h-[68px]" />

        {/* HEADER */}
        <div data-tauri-drag-region className={`v6-header-row relative z-10 flex shrink-0 select-none items-center px-2.5 pb-4 pt-1.5 ${brandOnLeft ? 'justify-between' : 'justify-end'}`}>
          {brandOnLeft ? (
            <div
              data-tauri-drag-region
              className={`v6-header-brand flex items-center gap-[11px] ${hasMainContent ? 'v6-brand-enter' : 'v6-brand-hidden'}`}
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
          ) : (
            <div data-tauri-drag-region className="pointer-events-none absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-[calc(50%+4px)]">
              <div
                data-tauri-drag-region
                className={`flex items-center gap-[11px] ${hasMainContent ? 'v6-brand-enter' : 'v6-brand-hidden'}`}
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
            </div>
          )}

          <div className="v6-header-actions flex items-center gap-2.5">
            {gbLeft !== null && (
              <div className="flex items-center gap-[9px] rounded-[30px] border border-white/[0.12] bg-white/[0.08] px-4 py-2">
                <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: chipColor, boxShadow: `0 0 8px ${chipColor}` }} />
                <span className="text-[13px] font-semibold text-white/90">
                  {gbLeft >= 100 ? Math.round(gbLeft) : gbLeft.toFixed(1)} <span className="font-normal text-white/50">{t('v6GbLeft' as never)}</span>
                </span>
              </div>
            )}
            <HeaderButton
              label={t((windowMode === 'compact' ? 'v6WindowWide' : 'v6WindowCompact') as never)}
              onClick={toggleWindowMode}
              disabled={windowTransitioning}
            >
              {windowMode === 'compact'
                ? <RectangleHorizontal className="h-5 w-5" strokeWidth={2} />
                : <RectangleVertical className="h-5 w-5" strokeWidth={2} />}
            </HeaderButton>
            <HeaderButton label={t('v6SupportTitle' as never)} onClick={() => setSupportOpen(true)}>
              <HelpCircle className="h-5 w-5" strokeWidth={2} />
            </HeaderButton>
            <HeaderButton label={t('settings' as never)} onClick={() => setSettingsOpen(true)}>
              <SlidersHorizontal className="h-5 w-5" strokeWidth={2} />
            </HeaderButton>
            {!nativeMacWindow && <WindowControls />}
          </div>
        </div>

        {/* CONTENT */}
        <main ref={contentRef} className={`relative z-10 flex min-h-0 flex-1 flex-col${windowTransitioning ? ' will-change-[transform,opacity]' : ''}`}>{children}</main>

        {/* OVERLAYS */}
        {supportOpen && (
          <SupportModal onClose={() => setSupportOpen(false)} onExportSupportBundle={exportSupportBundle} t={t} />
        )}
        {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} t={t} />}
      </div>
    </div>
  );
}

function HeaderButton({ children, onClick, label, disabled = false }: { children: ReactNode; onClick: () => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className="v6-header-button v6-hover-bright flex h-10 w-10 items-center justify-center rounded-[13px] border border-white/[0.12] bg-white/[0.07] text-white/[0.78] disabled:cursor-wait disabled:opacity-50 v6-focus"
    >
      {children}
    </button>
  );
}
