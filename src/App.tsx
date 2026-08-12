import { useEffect, useState, type ReactNode } from 'react';
import { ArrowLeft } from 'lucide-react';
import { isEnabled } from '@tauri-apps/plugin-autostart';
import AppShell from './components/v6/AppShell';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { flushPendingSecureWrites, useAppStore } from './stores/app-store';
import { useToastStore } from './stores/toast-store';
import { buildConnectRequestFromState } from './lib/connect-helpers';
import { isHealthAcceptable, summarizeHealthFailures, waitForConnectionHealth } from './lib/connection-health';
import { resolveConnectServer } from './lib/server-selection';
import { checkForAppUpdate, installAppUpdate, updatePhaseFromStatus } from './lib/app-updater';
import { buildAppConnectLocationRequestFromState, isClosedLocationServer } from './lib/app-control-plane';
import { isClosedControlPlaneEnabled, isDesktopAutostartAvailable, isNetworkExtensionOnlyBuild } from './lib/build-policy';
import { reportConnectionError } from './lib/workshop-api';
import { appRouteFromPathname, type AppRoute } from './lib/app-route';
import { desktopBridge } from './platform/tauri/desktop-bridge';
import './index.css';

const invoke = desktopBridge.command.bind(desktopBridge);

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

function NotificationStack() {
  return (
    <div className="fixed right-4 top-12 z-[9999] flex w-[min(320px,calc(100vw-6.5rem))] flex-col gap-2 pointer-events-none">
      <ToastContainer />
    </div>
  );
}

/**
 * Readable surface for pages that still use the retro (light-on-orange) design
 * inside the v6 glass shell. Adds a back-to-dashboard chip since the v6 shell
 * has no nav rail. Keeps pages fully functional until their own v6 reskin.
 */
function LegacySurface({ children, onBack }: { children: ReactNode; onBack: () => void }) {
  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[26px] bg-bg-primary text-text-on-orange">
      <button
        type="button"
        onClick={onBack}
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
  const [route, setRoute] = useState<AppRoute>(() =>
    appRouteFromPathname(typeof window === 'undefined' ? '/' : window.location.pathname)
  );

  useEffect(() => {
    const onPopState = () => setRoute(appRouteFromPathname(window.location.pathname));
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  const navigate = (next: AppRoute) => {
    if (window.location.pathname !== next) window.history.pushState(null, '', next);
    setRoute(next);
  };

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
      if (isNetworkExtensionOnlyBuild()) {
        try {
          useAppStore.setState({ autoStart: await invoke('app_store_autostart_enabled'), silentAdminAutostart: false });
        } catch (err) {
          useAppStore.setState({ autoStart: false, silentAdminAutostart: false });
          useAppStore.getState().addLog('debug', `App Store autostart is unavailable: ${err instanceof Error ? err.message : String(err)}`);
        }
        return;
      }
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
        const enabled = await isEnabled();
        useAppStore.setState({ autoStart: enabled });
      } catch (err) {
        useAppStore.setState({ autoStart: false });
        useAppStore.getState().addLog('warning', `App autostart status is unavailable: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    async function repairRuntimeOnStartup() {
      if (!isTauriRuntime() || isNetworkExtensionOnlyBuild()) return;
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
        
        const useClosedLocation = isClosedControlPlaneEnabled() && isClosedLocationServer(srv);
        const request = useClosedLocation
          ? await buildAppConnectLocationRequestFromState(srv)
          : await buildConnectRequestFromState(srv);
        const result = useClosedLocation
          ? await desktopBridge.appConnectLocation(request)
          : await desktopBridge.vpnConnect(request);
        
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
          await flushPendingSecureWrites();
        } finally {
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

  const page = route === '/servers' ? <LegacySurface onBack={() => navigate('/')}><Servers /></LegacySurface>
    : route === '/workshop' ? <LegacySurface onBack={() => navigate('/')}><Workshop /></LegacySurface>
      : route === '/settings' ? <LegacySurface onBack={() => navigate('/')}><Settings /></LegacySurface>
        : <Dashboard />;

  return (
    <>
      <AppShell>{page}</AppShell>
      <NotificationStack />
    </>
  );
}

export default App;
