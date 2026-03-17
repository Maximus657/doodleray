import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Sidebar } from './components/layout/Sidebar';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import { useToastStore } from './stores/toast-store';
import './index.css';

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
          useAppStore.getState().setAvailableUpdate(update.version);
          useToastStore.getState().addToast(`Update v${update.version} available!`, 'info');
        }
      } catch (e) {
        console.log('Update check skipped:', e);
      }
    }

    function autoConnectIfEnabled() {
      const { autoConnectOnStartup, servers, activeServer, status } = useAppStore.getState();
      if (!autoConnectOnStartup) return;
      if (status === 'connected' || status === 'connecting') return;
      if (!activeServer && servers.length === 0) return;
      
      // Wait for UI to fully render, then click connect
      setTimeout(() => {
        const btn = document.getElementById('connect-button');
        if (btn) btn.click();
      }, 2500);
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
    // Auto-connect if enabled
    autoConnectIfEnabled();

    return () => unsubscribe();
  }, []);

  return (
    <Router>
      <div className="flex h-screen bg-bg-primary">
        <Sidebar />
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/servers" element={<Servers />} />
          <Route path="/workshop" element={<Workshop />} />

          <Route path="/settings" element={<Settings />} />
        </Routes>
        <ToastContainer />
      </div>
    </Router>
  );
}

export default App;
