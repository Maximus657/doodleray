import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Download, Loader2 } from 'lucide-react';
import { Sidebar } from './components/layout/Sidebar';
import Dashboard from './pages/Dashboard';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import { useToastStore } from './stores/toast-store';
import { useWorkshopStore } from './stores/workshop-store';
import { buildConnectRequestFromState } from './lib/connect-helpers';
import './index.css';

// Cache the Update object so we don't call check() again when user clicks install.
// On macOS, calling check() multiple times can produce stale/duplicate objects.
let _cachedUpdate: any = null;
export function getCachedUpdate() { return _cachedUpdate; }
export function setCachedUpdate(u: any) { _cachedUpdate = u; }

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
  const availableUpdate = useAppStore((s) => s.availableUpdate);
  const [installing, setInstalling] = useState(false);

  if (!availableUpdate) return null;

  const handleInstall = async () => {
    setInstalling(true);
    try {
      // Reuse the cached Update object; only call check() if cache is empty
      let update = _cachedUpdate;
      if (!update) {
        const { check } = await import('@tauri-apps/plugin-updater');
        update = await check();
      }
      if (update) {
        await update.downloadAndInstall();
        _cachedUpdate = null;
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch();
      }
    } catch (e) {
      console.error('Update failed:', e);
      useToastStore.getState().addToast('Update failed — try again later', 'error');
      setInstalling(false);
    }
  };

  return (
    <div className="absolute top-0 left-0 right-0 z-50 animate-slide-down">
      <div className="mx-4 mt-3 bg-black border-[3px] border-black rounded-2xl px-5 py-3 flex items-center gap-4 shadow-[6px_6px_0_rgba(0,0,0,0.3)]">
        <div className="w-3 h-3 bg-red-500 rounded-full animate-pulse shrink-0" />
        <p className="flex-1 text-white text-xs font-black uppercase tracking-wide">
          🚀 New version <span className="text-bg-primary">v{availableUpdate}</span> is available!
        </p>
        <button
          onClick={handleInstall}
          disabled={installing}
          className="px-4 py-2 bg-bg-primary text-black border-[3px] border-white rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_rgba(255,255,255,0.3)] hover:-translate-y-0.5 hover:shadow-[5px_5px_0_rgba(255,255,255,0.3)] active:translate-y-1 active:shadow-none transition-all disabled:opacity-50 flex items-center gap-2 whitespace-nowrap"
        >
          {installing ? (
            <><Loader2 className="w-3.5 h-3.5 animate-spin" /> Updating...</>
          ) : (
            <><Download className="w-3.5 h-3.5 stroke-[3px]" /> Update Now</>
          )}
        </button>
      </div>
    </div>
  );
}

function App() {
  useEffect(() => {
    async function syncSilentAdmin() {
      try {
        const silentEnabled: boolean = await invoke('check_silent_autostart');
        useAppStore.setState({ silentAdminAutostart: silentEnabled });
      } catch (err) {
        console.error('Failed to query silent autostart:', err);
      }
    }

    async function checkForUpdates() {
      try {
        const { check } = await import('@tauri-apps/plugin-updater');
        const update = await check();
        if (update) {
          // Cache the Update object for one-click install later
          _cachedUpdate = update;
          const prev = useAppStore.getState().availableUpdate;
          useAppStore.getState().setAvailableUpdate(update.version);
          // Only show toast on fresh discovery (not repeated checks)
          if (!prev || prev !== update.version) {
            useToastStore.getState().addToast(`🚀 Update v${update.version} available!`, 'info');
          }
        }
      } catch (e) {
        console.log('Update check skipped:', e);
      }
    }

    async function autoConnectIfEnabled() {
      const state = useAppStore.getState();
      if (!state.autoConnectOnStartup) return;
      if (state.status === 'connected' || state.status === 'connecting') return;
      
      let srv = state.activeServer;
      if (!srv && state.servers.length > 0) {
        if (state.autoSelectFastest) {
          const withPing = state.servers.filter(s => s.ping !== undefined && s.ping > 0);
          srv = withPing.length > 0
            ? withPing.reduce((best, s) => (s.ping! < best.ping! ? s : best))
            : state.servers[0];
        } else {
          srv = state.servers[0];
        }
      }
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
        }
      } catch (err: any) {
        useAppStore.setState({ status: 'disconnected' });
        state.addLog('error', `Auto-connect error: ${err.message || err}`);
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

    syncSilentAdmin();
    // Check for updates after a brief delay so UI loads first
    setTimeout(checkForUpdates, 3000);
    // Re-check for updates every 30 minutes
    const updateInterval = setInterval(checkForUpdates, 30 * 60 * 1000);
    // Auto-connect if enabled
    autoConnectIfEnabled();
    
    // Analytics — report launch + start heartbeat
    import('./lib/workshop-api').then(({ reportLaunch, startHeartbeat }) => {
      reportLaunch();
      startHeartbeat();
    }).catch(() => { /* silent */ });

    // Hot-update routing rules when changed while VPN is connected (no restart needed)
    let rulesUpdateTimer: ReturnType<typeof setTimeout> | null = null;
    let prevRulesKey = JSON.stringify({
      r: useWorkshopStore.getState().myRules,
      p: useWorkshopStore.getState().appliedPresets,
    });
    const unsubRules = useWorkshopStore.subscribe(() => {
      const ws = useWorkshopStore.getState();
      const key = JSON.stringify({ r: ws.myRules, p: ws.appliedPresets });
      if (key === prevRulesKey) return;
      prevRulesKey = key;
      // Debounce 500ms
      if (rulesUpdateTimer) clearTimeout(rulesUpdateTimer);
      rulesUpdateTimer = setTimeout(async () => {
        const appState = useAppStore.getState();
        if (appState.status !== 'connected') return;
        // Get active rules and hot-update rule-set files on disk
        // sing-box watches these files and reloads automatically
        const { getActiveRoutingRules } = await import('./lib/connect-helpers');
        const rules = await getActiveRoutingRules();
        try {
          const result: any = await invoke('vpn_update_rules', { routingRules: rules });
          if (result.success) {
            appState.addLog('info', `Rules applied: ${result.message}`);
          } else {
            appState.addLog('error', result.message);
          }
        } catch (err: any) {
          appState.addLog('error', `Failed to update rules: ${err.message || err}`);
        }
      }, 500);
    });

    return () => {
      unsubscribe();
      unsubRules();
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
