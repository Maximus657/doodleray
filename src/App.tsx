import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Sidebar } from './components/layout/Sidebar';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import './index.css';

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
        }
      } catch (e) {
        console.log('Update check skipped:', e);
      }
    }

    syncSilentAdmin();
    // Check for updates after a brief delay so UI loads first
    setTimeout(checkForUpdates, 3000);
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
      </div>
    </Router>
  );
}

export default App;
