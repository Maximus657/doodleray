import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { isEnabled } from '@tauri-apps/plugin-autostart';
import { Sidebar } from './components/layout/Sidebar';
import Dashboard from './pages/Dashboard';
import Servers from './pages/Servers';
import Workshop from './pages/Workshop';

import Settings from './pages/Settings';
import { useAppStore } from './stores/app-store';
import './index.css';

function App() {
  const { alwaysRunAdmin } = useAppStore();

  useEffect(() => {
    async function checkPrivileges() {
      try {
        if (alwaysRunAdmin) {
          const isAdmin = await invoke<boolean>('is_admin');
          if (!isAdmin) {
            await invoke('restart_as_admin');
          }
        }
      } catch (err) {
        console.error('Failed to check or request admin privileges:', err);
      }
    }

    async function checkAutostart() {
      try {
        const enabled = await isEnabled();
        useAppStore.setState({ autoStart: enabled });
      } catch (err) {
        console.error('Failed to query autostart status:', err);
      }
    }

    checkPrivileges();
    checkAutostart();
  }, [alwaysRunAdmin]);

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
