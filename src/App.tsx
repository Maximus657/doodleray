import { BrowserRouter as Router, Routes, Route, useNavigate } from 'react-router-dom';
import { useEffect, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Download, Loader2, ArrowLeft } from 'lucide-react';
import { useTranslation } from './locales';
import AppShell from './components/v6/AppShell';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import { useToastStore } from './stores/toast-store';
import { buildConnectRequestFromState } from './lib/connect-helpers';
import { isHealthAcceptable, summarizeHealthFailures, waitForConnectionHealth } from './lib/connection-health';
import { resolveConnectServer } from './lib/server-selection';
import { checkForAppUpdate, installAppUpdate } from './lib/app-updater';
import { isInAppUpdateEnabled, openStoreUpdatePage } from './lib/update-channel';
import { buildAppConnectLocationRequestFromState, isClosedLocationServer } from './lib/app-control-plane';
import { isClosedControlPlaneEnabled, isDesktopAutostartAvailable } from './lib/build-policy';
import './index.css';

function formatMessage(template: string, values: Record<string, string | number>) {
  return Object.entries(values).reduce(
    (message, [key, value]) => message.replace(new RegExp(`\\{${key}\\}`, 'g'), String(value)),
    template
  );
}

function updatePhaseFromStatus(status: string): 'installing' | 'downloading' {
  return status === 'updateClosingProcesses' ||
    status === 'updatePreparingInstall' ||
    status === 'updateInstallingRestarting'
    ? 'installing'
    : 'downloading';
}

function updateStatusLabel(
  status: string,
  phase: string,
  progress: number | null,
  version: string | null,
  t: (key: any) => string,
) {
  if (progress !== null && phase === 'downloading') {
    return formatMessage(t('updateDownloadingProgress'), { progress });
  }

  switch (status) {
    case 'updateChecking':
      return t('updateChecking');
    case 'updateDownloading':
    case 'updateDownloadingProgress':
      return version
        ? formatMessage(t('updateDownloadingVersion'), { version })
        : t('updateDownloading');
    case 'updatePreparingInstall':
      return t('updatePreparingInstall');
    case 'updateClosingProcesses':
      return t('updateClosingProcesses');
    case 'updateInstallingRestarting':
      return t('updateInstallingRestarting');
    case 'updateOpenStore':
      return t('updateOpenStore');
    case 'updateLatest':
      return t('updateLatest');
    default:
      return status;
  }
}

function ToastContainer() {
  const toasts = useToastStore(s => s.toasts);
  const removeToast = useToastStore(s => s.removeToast);

  if (toasts.length === 0) return null;
  const accent = (type: string) =>
    type === 'success' ? '#3ddc84' : type === 'error' ? '#ff6b5a' : type === 'warning' ? '#ffb02e' : '#F97F16';
  return (
    <div className="flex flex-col gap-2 pointer-events-none">
      {toasts.map((t) => (
        <div
          key={t.id}
          onClick={() => removeToast(t.id)}
          className="v6-modal v6-fadein pointer-events-auto cursor-pointer rounded-2xl px-4 py-3 text-[12.5px] font-medium leading-snug text-white/90"
          style={{ borderLeft: `3px solid ${accent(t.type)}`, wordBreak: 'break-word' }}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}

function UpdateBanner() {
  const { t } = useTranslation();
  const availableUpdate = useAppStore((s) => s.availableUpdate);
  const updatePhase = useAppStore((s) => s.updatePhase);
  const updateStatus = useAppStore((s) => s.updateStatus);
  const updateProgress = useAppStore((s) => s.updateProgress);
  const setUpdateState = useAppStore((s) => s.setUpdateState);

  if (!availableUpdate) return null;

  const isDownloading = updatePhase === 'downloading';
  const isBusy = updatePhase === 'checking' || updatePhase === 'downloading' || updatePhase === 'installing';
  const statusLabel = updateStatusLabel(updateStatus, updatePhase, updateProgress, availableUpdate, t);
  const secondaryLabel = isBusy || updatePhase === 'error'
    ? statusLabel || t('updating')
    : null;
  const progressLabel = updateProgress !== null && isDownloading ? `${updateProgress}%` : null;

  const handleInstall = async () => {
    if (isBusy) return;
    // Mac App Store builds delegate installation to Apple; direct Windows
    // builds keep the signed in-app updater.
    if (!isInAppUpdateEnabled()) {
      await openStoreUpdatePage();
      setUpdateState({ updatePhase: 'available', updateStatus: 'updateOpenStore', updateProgress: null });
      return;
    }
    setUpdateState({
      updatePhase: 'downloading',
      updateStatus: 'updateDownloading',
      updateProgress: 0,
    });
    try {
      await installAppUpdate({
        onStatus: (status) => {
          setUpdateState({
            updateStatus: status,
            updatePhase: updatePhaseFromStatus(status),
          });
        },
        onProgress: (progress) => setUpdateState({ updateProgress: progress }),
      });
    } catch (e) {
      console.error('Update failed:', e);
      setUpdateState({
        updatePhase: 'error',
        updateStatus: t('updateFailed'),
        updateProgress: null,
      });
      useToastStore.getState().addToast(t('updateFailed'), 'error');
    }
  };

  return (
    <div className="pointer-events-auto w-full animate-slide-in-right rounded-2xl border-[3px] border-black bg-black px-4 py-3 text-white shadow-[5px_5px_0_rgba(0,0,0,0.28)]">
      <div className="flex items-start gap-3">
        <div className={`mt-1.5 h-3 w-3 shrink-0 rounded-full ${isBusy ? 'animate-pulse bg-amber-300' : 'bg-emerald-400'}`} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <p className="text-[12px] font-black uppercase leading-tight tracking-[0.14em] text-white">
              {t('newUpdate')} v<span className="text-bg-primary">{availableUpdate}</span>
            </p>
            {!isBusy && (
              <span className="rounded-full bg-bg-primary px-2 py-0.5 text-[9px] font-black uppercase tracking-widest text-black">
                {t('versionAvailable')}
              </span>
            )}
          </div>
          {secondaryLabel && (
            <p className="mt-1 text-[10px] font-black uppercase leading-snug tracking-widest text-white/65">
              {secondaryLabel}
            </p>
          )}
        </div>
        {progressLabel && (
          <span className="shrink-0 rounded-lg border-2 border-white bg-bg-primary px-2 py-0.5 text-[10px] font-black tabular-nums tracking-wider text-black">
            {progressLabel}
          </span>
        )}
      </div>

      {(isBusy || updateProgress !== null) && (
        <div className="mt-2.5 h-2 overflow-hidden rounded-full border-2 border-white bg-white/15">
          {updateProgress !== null && isDownloading ? (
            <div className="h-full bg-bg-primary transition-all duration-300" style={{ width: `${updateProgress}%` }} />
          ) : (
            <div className="h-full w-1/2 animate-pulse rounded-full bg-bg-primary" />
          )}
        </div>
      )}

      <button
        onClick={handleInstall}
        disabled={isBusy}
        className="mt-2 flex w-full cursor-pointer items-center justify-center gap-2 rounded-xl border-2 border-white bg-bg-primary px-3 py-2 text-[10px] font-black uppercase tracking-widest text-black shadow-[3px_3px_0_rgba(255,255,255,0.3)] transition-all hover:-translate-y-0.5 hover:shadow-[5px_5px_0_rgba(255,255,255,0.3)] active:translate-y-1 active:shadow-none disabled:cursor-wait disabled:opacity-75"
      >
        {isBusy ? (
          <><Loader2 className="h-3.5 w-3.5 animate-spin" /> {progressLabel ? `${t('updateDownloading')} ${progressLabel}` : t('updating')}</>
        ) : (
          <><Download className="h-3.5 w-3.5 stroke-[3px]" /> {isInAppUpdateEnabled() ? t('installRestart') : t('updateOpenStore')}</>
        )}
      </button>
    </div>
  );
}

function NotificationStack() {
  return (
    <div className="fixed right-4 top-12 z-[9999] flex w-[min(320px,calc(100vw-6.5rem))] flex-col gap-2 pointer-events-none">
      <UpdateBanner />
      <ToastContainer />
    </div>
  );
}

/**
 * Readable surface for pages that still use the retro (light-on-orange) design
 * inside the v6 glass shell. Adds a back-to-dashboard chip since the v6 shell
 * has no nav rail. Keeps pages fully functional until their own v6 reskin.
 */
function LegacySurface({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[26px] bg-bg-primary text-text-on-orange">
      <button
        type="button"
        onClick={() => navigate('/')}
        aria-label="Back"
        className="absolute left-4 top-4 z-40 flex h-9 items-center gap-1.5 rounded-xl border-[3px] border-black bg-white px-2.5 text-[11px] font-black uppercase tracking-widest text-black shadow-[3px_3px_0_#000] transition-all hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none"
      >
        <ArrowLeft className="h-4 w-4 stroke-[3px]" />
      </button>
      {children}
    </div>
  );
}

function App() {
  useEffect(() => {
    const isTauriRuntime = () => {
      if (typeof window === 'undefined') return false;
      const tauriInternals = (window as unknown as {
        __TAURI_INTERNALS__?: { invoke?: unknown };
      }).__TAURI_INTERNALS__;
      return typeof tauriInternals?.invoke === 'function';
    };

    async function syncStartupAutostart() {
      if (!isTauriRuntime()) return;
      if (!isDesktopAutostartAvailable()) {
        useAppStore.setState({ autoStart: false, silentAdminAutostart: false });
        return;
      }

      let silentEnabled = false;
      try {
        silentEnabled = await invoke('check_silent_autostart');
        useAppStore.setState({ silentAdminAutostart: silentEnabled });
      } catch (err) {
        console.error('Failed to query silent autostart:', err);
      }

      if (silentEnabled) {
        useAppStore.setState({ autoStart: false });
        return;
      }

      try {
        const { isEnabled } = await import('@tauri-apps/plugin-autostart');
        const enabled = await isEnabled();
        useAppStore.setState({ autoStart: enabled });
      } catch (err) {
        useAppStore.setState({ autoStart: false });
        useAppStore.getState().addLog('warning', `App autostart status is unavailable: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    async function repairRuntimeOnStartup() {
      if (!isTauriRuntime()) return;
      try {
        const message = await invoke('repair_windows_runtime');
        const firstLine = typeof message === 'string' ? message.split('\n')[0] : null;
        if (firstLine) {
          useAppStore.getState().addLog('debug', `Startup repair: ${firstLine}`);
        }
      } catch (err) {
        useAppStore.getState().addLog('debug', `Startup repair skipped: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    async function checkForUpdates(options: { autoInstall?: boolean; silent?: boolean } = {}) {
      try {
        const currentVersion = await import('@tauri-apps/api/app')
          .then(({ getVersion }) => getVersion())
          .catch(() => 'unknown');
        if (!options.silent) {
          useAppStore.getState().addLog('info', `Checking for app update (current v${currentVersion})...`);
        }
        const update = await checkForAppUpdate();
        if (update) {
          const prev = useAppStore.getState().availableUpdate;
          useAppStore.getState().setUpdateState({
            availableUpdate: update.version,
            updatePhase: 'available',
            updateStatus: '',
            updateProgress: null,
          });
          if (!options.silent || !prev || prev !== update.version) {
            useAppStore.getState().addLog('info', `App update available: v${currentVersion} -> v${update.version}`);
          }

          if (options.autoInstall) {
            useAppStore.getState().addLog('info', `Auto-update found v${update.version}. Installing...`);
            useAppStore.getState().setUpdateState({
              updatePhase: 'downloading',
              updateStatus: 'updateDownloading',
              updateProgress: 0,
            });
            await installAppUpdate({
              update,
              onStatus: (status) => {
                useAppStore.getState().setUpdateState({
                  updateStatus: status,
                  updatePhase: updatePhaseFromStatus(status),
                });
                useAppStore.getState().addLog('info', `Auto-update: ${status}`);
              },
              onProgress: (progress) => useAppStore.getState().setUpdateState({ updateProgress: progress }),
            });
            return true;
          }

          if (!prev || prev !== update.version) {
            useAppStore.getState().addLog('info', `Showing app update banner for v${update.version}`);
          }
          return false;
        }

        useAppStore.getState().setUpdateState({
          availableUpdate: null,
          updatePhase: 'idle',
          updateStatus: '',
          updateProgress: null,
        });
        if (!options.silent) {
          useAppStore.getState().addLog('success', `App is up to date (v${currentVersion})`);
        }
        return false;
      } catch (e) {
        console.log('Update check skipped:', e);
        const message = e instanceof Error ? e.message : String(e);
        if (!options.silent) {
          useAppStore.getState().setUpdateState({
            updatePhase: 'error',
            updateStatus: message,
            updateProgress: null,
          });
          useAppStore.getState().addLog('warning', `App update check failed: ${message}`);
        }
        return false;
      }
    }

    async function autoConnectIfEnabled() {
      const state = useAppStore.getState();
      if (!state.autoConnectOnStartup) return;
      if (isClosedControlPlaneEnabled() && state.appSessionDeviceAllowed !== true) return;
      if (state.status === 'connected' || state.status === 'connecting') return;
      
      const srv = resolveConnectServer(state.activeServer, state.servers, state.autoSelectFastest);
      if (!srv) return;

      // Prevent concurrent executions by setting status immediately before the sleep delay
      useAppStore.setState({ status: 'connecting', activeServer: srv });

      await new Promise(r => setTimeout(r, 2000));
      
      try {
        state.addLog('info', `Auto-connecting to ${srv.name}...`);
        
        const { invoke } = await import('@tauri-apps/api/core');
        const useClosedLocation = isClosedControlPlaneEnabled() && isClosedLocationServer(srv);
        const request = useClosedLocation
          ? await buildAppConnectLocationRequestFromState(srv)
          : await buildConnectRequestFromState(srv);
        const result: any = await invoke(useClosedLocation ? 'app_connect_location' : 'vpn_connect', { request });
        
        if (result.success) {
          const { health } = await waitForConnectionHealth(
            invoke,
            state.proxyMode,
            request.system_proxy_mode,
            request.socks_port,
            request.http_port,
            result.health ?? null,
          );
          if (isHealthAcceptable(state.proxyMode, health)) {
            useAppStore.setState({ status: 'connected', connectedAt: Date.now() });
            state.addLog('success', `Auto-connected to ${srv.name}`);
          } else {
            await invoke('vpn_disconnect').catch(() => undefined);
            useAppStore.setState({ status: 'disconnected' });
            state.addLog('error', `Auto-connect health check failed: ${summarizeHealthFailures(health)}`);
          }
        } else {
          useAppStore.setState({ status: 'disconnected' });
          state.addLog('error', `Auto-connect failed: ${result.message}`);
          const { reportConnectionError } = await import('./lib/workshop-api');
          reportConnectionError({
            eventType: 'connect_fail',
            serverName: srv.name,
            serverAddress: srv.address,
            serverPort: srv.port,
            protocol: srv.protocol,
            errorMessage: result.message,
            details: { action: 'auto_connect' },
          });
        }
      } catch (err: any) {
        useAppStore.setState({ status: 'disconnected' });
        const message = err.message || String(err);
        state.addLog('error', `Auto-connect error: ${message}`);
        const { reportConnectionError } = await import('./lib/workshop-api');
        reportConnectionError({
          eventType: 'connect_fail',
          serverName: srv.name,
          serverAddress: srv.address,
          serverPort: srv.port,
          protocol: srv.protocol,
          errorMessage: message,
          details: { action: 'auto_connect_exception' },
        });
      }
    }

    // Subscribe to status changes for toast notifications
    const unsubscribe = useAppStore.subscribe(
      (state, prevState) => {
        if (prevState.status === 'connected' && state.status === 'disconnected') {
          useToastStore.getState().addToast('VPN Disconnected', 'warning');
        }
      }
    );

    let unsubscribeHydration: (() => void) | undefined;
    let startupFlowTimer: ReturnType<typeof setTimeout> | undefined;
    let updateInProgress = false;

    function compactHydratedStorage() {
      const state = useAppStore.getState();
      useAppStore.setState({
        subscriptions: state.subscriptions.map((sub) => ({ ...sub, servers: [] })),
      });
    }

    syncStartupAutostart();

    const runStartupFlow = () => {
      compactHydratedStorage();
      startupFlowTimer = setTimeout(async () => {
        await repairRuntimeOnStartup();
        updateInProgress = true;
        const installingUpdate = await checkForUpdates({ autoInstall: false, silent: true });
        updateInProgress = false;
        if (installingUpdate) return;
        autoConnectIfEnabled();
      }, 3000);
    };

    // Re-check for updates every 30 minutes and keep the install as an explicit user action.
    const updateInterval = setInterval(async () => {
      if (updateInProgress) return;
      updateInProgress = true;
      try {
        await checkForUpdates({ autoInstall: false, silent: true });
      } finally {
        updateInProgress = false;
      }
    }, 30 * 60 * 1000);

    // Auto-connect needs persisted servers/settings to be loaded first.
    if (useAppStore.persist.hasHydrated()) {
      runStartupFlow();
    } else {
      unsubscribeHydration = useAppStore.persist.onFinishHydration(() => {
        runStartupFlow();
      });
    }
    
    return () => {
      unsubscribe();
      unsubscribeHydration?.();
      if (startupFlowTimer) clearTimeout(startupFlowTimer);
      clearInterval(updateInterval);
    };
  }, []);

  useEffect(() => {
    // A Settings change (e.g. a custom port) persists via an async
    // secure-storage write. Quitting right after committing one could tear
    // the process down mid round-trip and silently drop it back to the
    // default on next launch. The Rust exit handler holds the process open
    // briefly and waits for this confirmation instead of exiting immediately.
    let unlisten: (() => void) | undefined;
    let disposed = false;
    import('@tauri-apps/api/event').then(({ listen }) =>
      listen('doodleray:flush-before-exit', async () => {
        try {
          const { flushPendingSecureWrites } = await import('./stores/app-store');
          await flushPendingSecureWrites();
        } finally {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('confirm_secure_storage_flushed').catch(() => undefined);
        }
      })
    ).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <Router>
      <AppShell>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/servers" element={<LegacySurface><Servers /></LegacySurface>} />
          <Route path="/workshop" element={<LegacySurface><Workshop /></LegacySurface>} />
          <Route path="/settings" element={<LegacySurface><Settings /></LegacySurface>} />
        </Routes>
      </AppShell>
      <NotificationStack />
    </Router>
  );
}

export default App;
