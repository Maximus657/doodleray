import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Download, Loader2 } from 'lucide-react';
import { useTranslation } from './locales';
import { Sidebar } from './components/layout/Sidebar';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import { useToastStore } from './stores/toast-store';
import { buildConnectRequestFromState } from './lib/connect-helpers';
import { resolveConnectServer } from './lib/server-selection';
import { checkForAppUpdate, installAppUpdate } from './lib/app-updater';
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
  return (
    <div className="fixed top-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none">
      {toasts.map((t) => (
        <div key={t.id}
          onClick={() => removeToast(t.id)}
          className={`pointer-events-auto px-5 py-3 rounded-xl border-[3px] border-black shadow-[4px_4px_0_#000] font-black text-sm uppercase tracking-tight cursor-pointer
            animate-slide-up transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000]
            ${t.type === 'success' ? 'bg-emerald-400 text-black' :
              t.type === 'error' ? 'bg-danger text-white' :
              t.type === 'warning' ? 'bg-amber-400 text-black' :
              'bg-white text-black'}`}>
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

  const handleInstall = async () => {
    if (isBusy) return;
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
    <div className="fixed bottom-4 right-4 z-[9000] w-[min(420px,calc(100vw-2rem))] animate-slide-in-right pointer-events-none">
      <div className="pointer-events-auto overflow-hidden rounded-2xl border-[3px] border-black bg-black px-4 py-3 shadow-[6px_6px_0_rgba(0,0,0,0.3)]">
        <div className="flex items-center gap-3">
          <div className={`h-3 w-3 shrink-0 rounded-full ${isBusy ? 'animate-pulse bg-amber-300' : 'bg-emerald-400'}`} />
          <div className="min-w-0 flex-1">
            <p className="truncate text-xs font-black uppercase tracking-wide text-white">
              {t('newUpdate')} v<span className="text-bg-primary">{availableUpdate}</span> {t('versionAvailable')}
            </p>
            {(statusLabel || isDownloading) && (
              <p className="mt-0.5 truncate text-[10px] font-black uppercase tracking-widest text-white/55">
                {statusLabel}
              </p>
            )}
          </div>
          <button
            onClick={handleInstall}
            disabled={isBusy}
            className="flex cursor-pointer items-center gap-2 whitespace-nowrap rounded-xl border-[3px] border-white bg-bg-primary px-4 py-2 text-[10px] font-black uppercase tracking-widest text-black shadow-[3px_3px_0_rgba(255,255,255,0.3)] transition-all hover:-translate-y-0.5 hover:shadow-[5px_5px_0_rgba(255,255,255,0.3)] active:translate-y-1 active:shadow-none disabled:cursor-wait disabled:opacity-70"
          >
            {isBusy ? (
              <><Loader2 className="h-3.5 w-3.5 animate-spin" /> {updateProgress !== null && isDownloading ? `${updateProgress}%` : t('updating')}</>
            ) : (
              <><Download className="h-3.5 w-3.5 stroke-[3px]" /> {t('installRestart')}</>
            )}
          </button>
        </div>
        {updateProgress !== null && isDownloading && (
          <div className="mt-3 h-2 overflow-hidden rounded-full border-[2px] border-white/80 bg-white/15">
            <div className="h-full bg-bg-primary transition-all duration-300" style={{ width: `${updateProgress}%` }} />
          </div>
        )}
      </div>
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
        const { enable, isEnabled } = await import('@tauri-apps/plugin-autostart');
        const enabled = await isEnabled();
        if (!enabled) {
          await enable();
          useAppStore.getState().addLog('success', 'App autostart enabled');
        }
        useAppStore.setState({ autoStart: true });
      } catch (err) {
        useAppStore.setState({ autoStart: false });
        useAppStore.getState().addLog('warning', `App autostart could not be enabled: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    async function checkForUpdates(options: { autoInstall?: boolean } = {}) {
      try {
        const currentVersion = await import('@tauri-apps/api/app')
          .then(({ getVersion }) => getVersion())
          .catch(() => 'unknown');
        useAppStore.getState().addLog('info', `Checking for app update (current v${currentVersion})...`);
        const update = await checkForAppUpdate();
        if (update) {
          const prev = useAppStore.getState().availableUpdate;
          useAppStore.getState().setUpdateState({
            availableUpdate: update.version,
            updatePhase: 'available',
            updateStatus: '',
            updateProgress: null,
          });
          useAppStore.getState().addLog('info', `App update available: v${currentVersion} -> v${update.version}`);

          if (options.autoInstall) {
            useToastStore.getState().addToast(`Installing update v${update.version}...`, 'info');
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

          // Only show toast on fresh discovery (not repeated checks)
          if (!prev || prev !== update.version) {
            useToastStore.getState().addToast(`Update v${update.version} available`, 'info');
          }
          return false;
        }

        useAppStore.getState().setUpdateState({
          availableUpdate: null,
          updatePhase: 'idle',
          updateStatus: '',
          updateProgress: null,
        });
        useAppStore.getState().addLog('success', `App is up to date (v${currentVersion})`);
        return false;
      } catch (e) {
        console.log('Update check skipped:', e);
        const message = e instanceof Error ? e.message : String(e);
        useAppStore.getState().setUpdateState({
          updatePhase: 'error',
          updateStatus: message,
          updateProgress: null,
        });
        useAppStore.getState().addLog('warning', `App update check failed: ${message}`);
        return false;
      }
    }

    async function autoConnectIfEnabled() {
      const state = useAppStore.getState();
      if (!state.autoConnectOnStartup) return;
      if (state.status === 'connected' || state.status === 'connecting') return;
      
      const srv = resolveConnectServer(state.activeServer, state.servers, state.autoSelectFastest);
      if (!srv) return;

      // Prevent concurrent executions by setting status immediately before the sleep delay
      useAppStore.setState({ status: 'connecting', activeServer: srv });

      await new Promise(r => setTimeout(r, 2000));
      
      try {
        state.addLog('info', `Auto-connecting to ${srv.name}...`);
        
        const { invoke } = await import('@tauri-apps/api/core');
        const request = await buildConnectRequestFromState(srv);
        const result: any = await invoke('vpn_connect', { request });
        
        if (result.success) {
          useAppStore.setState({ status: 'connected', connectedAt: Date.now() });
          state.addLog('success', `Auto-connected to ${srv.name}`);
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
        if (prevState.status === 'connecting' && state.status === 'connected') {
          useToastStore.getState().addToast('VPN Connected ✓', 'success');
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
        updateInProgress = true;
        const installingUpdate = await checkForUpdates({ autoInstall: false });
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
        await checkForUpdates({ autoInstall: false });
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
    
    // Analytics — report launch, update event, and heartbeat
    import('./lib/workshop-api').then(async ({ reportLaunch, startHeartbeat, reportAppUpdated }) => {
      reportLaunch();
      startHeartbeat();
      try {
        const { getVersion } = await import('@tauri-apps/api/app');
        const currentVersion = await getVersion();
        const previousVersion = localStorage.getItem('doodleray_last_seen_version');
        if (previousVersion && previousVersion !== currentVersion) {
          reportAppUpdated(previousVersion, currentVersion);
        }
        localStorage.setItem('doodleray_last_seen_version', currentVersion);
      } catch {
        // Version analytics must never affect startup.
      }
    }).catch(() => { /* silent */ });

    return () => {
      unsubscribe();
      unsubscribeHydration?.();
      if (startupFlowTimer) clearTimeout(startupFlowTimer);
      clearInterval(updateInterval);
    };
  }, []);

  return (
    <Router>
      <div className="flex h-screen bg-bg-primary">
        <Sidebar />
        <div className="flex-1 flex flex-col relative overflow-hidden">
          <UpdateBanner />
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/servers" element={<Servers />} />
            <Route path="/workshop" element={<Workshop />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </div>
        <ToastContainer />
      </div>
    </Router>
  );
}

export default App;
