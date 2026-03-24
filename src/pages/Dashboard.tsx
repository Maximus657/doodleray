import { useState, useEffect, useCallback, useRef } from 'react';
import { Plus, Loader2, ClipboardPaste } from 'lucide-react';
import { useAppStore } from '../stores/app-store';
import { formatTime, pingServerSmart } from '../lib/utils';
import { refreshSubscription, fetchSubscription } from '../lib/subscription';
import { parseProxyLink } from '../lib/parser';
import { useTranslation } from '../locales';
import { reportConnectionError } from '../lib/workshop-api';
import { buildConnectRequestFromState } from '../lib/connect-helpers';

// Sub-components
import RetroBackground from '../components/dashboard/RetroBackground';
import OnboardingCard from '../components/dashboard/OnboardingCard';
import ConnectionControls from '../components/dashboard/ConnectionControls';
import StatsPanel from '../components/dashboard/StatsPanel';
import ServerList from '../components/dashboard/ServerList';
import LogsStrip from '../components/dashboard/LogsStrip';

export default function Dashboard() {
  const {
    status, setStatus, activeServer, servers, setActiveServer,
    proxyMode, setProxyMode, speedHistory, currentDownload, currentUpload,
    totalDown, totalUp, addTraffic, resetTraffic, addSpeedPoint, setCurrentSpeed,
    logs, addLog, clearLogs, socksPort, httpPort, subscriptions,
    updateSubscription, removeSubscription, autoSelectFastest,
    subAutoUpdateMinutes, connectedAt, setConnectedAt,
    addSubscription, addServer, removeServer, removeAllManualServers,
    updateServerPing, showStats,
  } = useAppStore();
  const { t } = useTranslation();

  const [showLogs, setShowLogs] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [quickInput, setQuickInput] = useState('');
  const [quickImporting, setQuickImporting] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [testingSubId, setTestingSubId] = useState<string | null>(null);
  const [refreshingSubId, setRefreshingSubId] = useState<string | null>(null);
  const [pingingServerId, setPingingServerId] = useState<string | null>(null);

  // ═══════════════════════════════════════════════════
  //  Effects
  // ═══════════════════════════════════════════════════

  // Auto-ping unpinged servers on mount
  useEffect(() => {
    const unpinged = servers.filter(s => s.ping === undefined);
    if (unpinged.length === 0) return;
    let cancelled = false;
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        for (const server of unpinged) {
          if (cancelled) break;
          try {
            const ping = await pingServerSmart(server, invoke);
            updateServerPing(server.id, ping);
          } catch { updateServerPing(server.id, -1); }
          await new Promise(r => setTimeout(r, 30));
        }
      } catch { /* not in tauri env */ }
    })();
    return () => { cancelled = true; };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Connection time counter
  const [connectTime, setConnectTime] = useState(0);
  useEffect(() => {
    if (status !== 'connected' || !connectedAt) { setConnectTime(0); return; }
    setConnectTime(Math.floor((Date.now() - connectedAt) / 1000));
    const interval = setInterval(() => {
      setConnectTime(Math.floor((Date.now() - connectedAt) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [status, connectedAt]);

  // Sync connection state from backend on mount
  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const running: boolean = await invoke('vpn_status');
        if (running && status !== 'connected') {
          setStatus('connected');
          addLog('info', 'VPN is still active (reconnected after UI reload)');
        }
      } catch { /* not in tauri env */ }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Connection health monitor
  const healthFailRef = useRef(0);
  useEffect(() => {
    if (status !== 'connected') { healthFailRef.current = 0; return; }
    const healthCheck = setInterval(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const healthy: boolean = await invoke('check_connection_health', { socksPort });
        if (healthy) { healthFailRef.current = 0; }
        else {
          healthFailRef.current++;
          if (healthFailRef.current >= 3) {
            addLog('warning', 'Connection lost — SOCKS port not responding. Auto-reconnecting...');
            const { useToastStore } = await import('../stores/toast-store');
            useToastStore.getState().addToast('Connection lost — reconnecting...', 'warning');
            const currentServer = useAppStore.getState().activeServer;
            reportConnectionError({
              eventType: 'health_drop', serverName: currentServer?.name,
              serverAddress: currentServer?.address, serverPort: currentServer?.port,
              protocol: currentServer?.protocol,
              errorMessage: 'SOCKS port not responding (3 consecutive health-check failures)',
            });
            try {
              await invoke('vpn_disconnect');
              setStatus('disconnected');
              healthFailRef.current = 0;
              setTimeout(() => { document.getElementById('connect-button')?.click(); }, 2000);
            } catch { /* ignore */ }
          }
        }
      } catch { /* not in tauri env */ }
    }, 30000);
    return () => clearInterval(healthCheck);
  }, [status, socksPort]); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-scroll logs
  useEffect(() => {
    if (showLogs) logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs.length, showLogs]);

  // Poll xray-core proxy logs
  useEffect(() => {
    if (status !== 'connected') return;
    const poll = setInterval(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const lines: string[] = await invoke('get_proxy_logs');
        for (const line of lines) {
          if (!line.trim() || line.match(/tunneling request to tcp|accepted (?:tcp|udp)/)) continue;
          if (line.includes('[Warning]') || line.includes('[Error]') || line.includes('Failed')) {
            addLog(line.includes('[Error]') || line.includes('Failed') ? 'error' : 'warning', line);
          }
        }
      } catch { /* */ }
    }, 1000);
    return () => clearInterval(poll);
  }, [status, addLog]);

  // Poll traffic stats
  useEffect(() => {
    if (status !== 'connected') return;
    const interval = setInterval(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const stats: any = await invoke('get_traffic_stats');
        const dl = stats.download || 0;
        const ul = stats.upload || 0;
        setCurrentSpeed(dl, ul);
        addTraffic(dl, ul);
        addSpeedPoint({ time: formatTime(new Date()), download: dl / 1024, upload: ul / 1024 });
      } catch { /* */ }
    }, 1000);
    return () => clearInterval(interval);
  }, [status, addSpeedPoint, setCurrentSpeed, addTraffic]);

  // Auto-detect clipboard links
  const lastSuggestedRef = useRef<string>('');
  useEffect(() => {
    const checkClipboard = async () => {
      try {
        const text = await navigator.clipboard.readText();
        const trimmed = text.trim();
        if (/^(vless|vmess|trojan|ss|hy2|tuic|wg):\/\//.test(trimmed) || trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
          if (lastSuggestedRef.current !== trimmed) {
            lastSuggestedRef.current = trimmed;
            setQuickInput(trimmed);
            const { useToastStore } = await import('../stores/toast-store');
            if (useAppStore.getState().servers.length > 0) setShowAddModal(true);
            useToastStore.getState().addToast('Clipboard key detected! Ready to connect.', 'success');
            addLog('info', 'Found key in clipboard and pre-filled the input.');
          }
        }
      } catch { /* ignore */ }
    };
    window.addEventListener('focus', checkClipboard);
    const timer = setTimeout(checkClipboard, 1500);
    return () => { window.removeEventListener('focus', checkClipboard); clearTimeout(timer); };
  }, [addLog]);

  // Subscription auto-update
  useEffect(() => {
    if (subAutoUpdateMinutes <= 0 || subscriptions.length === 0) return;
    const interval = setInterval(async () => {
      for (const sub of subscriptions) {
        try {
          const updated = await refreshSubscription(sub);
          updateSubscription(sub.id, updated);
          addLog('info', `Auto-updated subscription: ${sub.name} (${updated.servers.length} servers)`);
        } catch { /* silently skip */ }
      }
    }, subAutoUpdateMinutes * 60 * 1000);
    return () => clearInterval(interval);
  }, [subAutoUpdateMinutes, subscriptions, updateSubscription, addLog]);

  // ═══════════════════════════════════════════════════
  //  Handlers
  // ═══════════════════════════════════════════════════

  const handleConnect = useCallback(async () => {
    if (status === 'disconnected') {
      let srv = activeServer;
      if (!srv && servers.length > 0) {
        if (autoSelectFastest) {
          const withPing = servers.filter(s => s.ping !== undefined && s.ping > 0);
          srv = withPing.length > 0 ? withPing.reduce((best, s) => (s.ping! < best.ping! ? s : best)) : servers[0];
          addLog('info', `Auto-selected fastest: ${srv.name} (${srv.ping}ms)`);
        } else { srv = servers[0]; }
        setActiveServer(srv);
      }
      if (!srv) { addLog('error', 'No server selected. Please add a subscription or select a server.'); return; }
      setStatus('connecting');

      // TUN mode admin check
      if (proxyMode === 'tun') {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const admin: boolean = await invoke('is_admin');
          if (!admin) {
            addLog('warning', 'TUN mode requires administrator privileges. Restarting...');
            try { await invoke('restart_as_admin'); return; }
            catch {
              addLog('error', 'Could not restart as admin — switching to System Proxy mode');
              const { useToastStore } = await import('../stores/toast-store');
              useToastStore.getState().addToast('Admin required for TUN mode — switched to System Proxy', 'warning');
              setProxyMode('system-proxy'); setStatus('disconnected'); return;
            }
          }
        } catch { /* */ }
      }

      setConnectedAt(null);
      addLog('info', `Starting connection to ${srv.name}...`);

      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const request = await buildConnectRequestFromState(srv);
        const result: any = await invoke('vpn_connect', { request });

        if (result.success) {
          addLog('success', result.message);
          addLog('success', `${t('protectedWorking')} ✅`);
          setStatus('connected'); setConnectedAt(Date.now());
        } else {
          // Port-busy retry
          if (result.message.includes('bind') || result.message.includes('10808') || result.message.includes('port')) {
            try {
              const portInfo: any = await invoke('check_port', { port: socksPort });
              if (portInfo.busy) {
                addLog('warning', 'Fixing connection route automatically...');
                await invoke('force_free_port', { port: socksPort });
                await new Promise(r => setTimeout(r, 1000));
                const retryReq = await buildConnectRequestFromState(srv!);
                const retry: any = await invoke('vpn_connect', { request: retryReq });
                if (retry.success) { addLog('success', retry.message); setStatus('connected'); setConnectedAt(Date.now()); return; }
              }
            } catch {}
          }
          addLog('error', result.message);
          reportConnectionError({ eventType: 'connect_fail', serverName: srv!.name, serverAddress: srv!.address, serverPort: srv!.port, protocol: srv!.protocol, errorMessage: result.message });
          setStatus('disconnected');
        }
      } catch (err: any) {
        addLog('warning', `Dev mode — simulating connection: ${err.message || err}`);
        setTimeout(() => { addLog('success', `[SIM] Connected via ${srv!.protocol.toUpperCase()}+${srv!.transport}`); setStatus('connected'); setConnectedAt(Date.now()); }, 1500);
      }
    } else if (status === 'connected') {
      addLog('warning', 'Disconnecting...');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result: any = await invoke('vpn_disconnect');
        addLog(result.success ? 'info' : 'error', result.message);
      } catch { addLog('info', '[SIM] Disconnected'); }
      setStatus('disconnected'); setCurrentSpeed(0, 0); resetTraffic();
    }
  }, [status, setStatus, setCurrentSpeed, resetTraffic, activeServer, servers, setActiveServer, addLog, proxyMode, socksPort, httpPort, autoSelectFastest, setConnectedAt, t, setProxyMode]);

  const handleModeSwitch = useCallback(async (mode: 'system-proxy' | 'tun') => {
    if (proxyMode === mode) return;
    if (mode === 'tun') {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const admin: boolean = await invoke('is_admin');
        if (!admin) {
          addLog('warning', 'TUN mode requires admin. Disconnecting and restarting as admin...');
          if (status === 'connected') await invoke('vpn_disconnect');
          setProxyMode(mode);
          try { await invoke('restart_as_admin'); return; }
          catch { addLog('error', 'Could not restart as admin — staying on System Proxy');
            const { useToastStore } = await import('../stores/toast-store');
            useToastStore.getState().addToast('Admin required for TUN mode', 'warning');
            setProxyMode('system-proxy'); return;
          }
        }
      } catch { /* */ }
    }
    setProxyMode(mode);
    addLog('info', `Switched routing mode to ${mode === 'tun' ? 'TUN' : 'System Proxy'}`);
    if (status === 'connected') {
      addLog('warning', 'Reconnecting to apply new routing mode...');
      setStatus('connecting');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        await new Promise(r => setTimeout(r, 2000));
        const srv = activeServer;
        if (srv) {
          const request = await buildConnectRequestFromState(srv, mode);
          const result: any = await invoke('vpn_connect', { request });
          if (result.success) { addLog('success', result.message); setStatus('connected'); setConnectedAt(Date.now()); }
          else { addLog('error', result.message); setStatus('disconnected'); }
        } else { setStatus('disconnected'); }
      } catch (err: any) { addLog('error', `Reconnect failed: ${err.message || err}`); setStatus('disconnected'); }
    }
  }, [proxyMode, setProxyMode, status, setStatus, addLog, activeServer, socksPort, httpPort, setConnectedAt]);

  const handleServerSelect = useCallback(async (server: typeof activeServer) => {
    if (!server) return;
    const isSameServer = activeServer?.id === server.id;
    setActiveServer(server); setSearchQuery('');
    if (status === 'connected' && !isSameServer) {
      addLog('warning', `Switching to ${server.name}...`);
      setStatus('connecting');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        await new Promise(r => setTimeout(r, 2000));
        const request = await buildConnectRequestFromState(server);
        const result: any = await invoke('vpn_connect', { request });
        if (result.success) { addLog('success', result.message); setStatus('connected'); setConnectedAt(Date.now()); }
        else { addLog('error', result.message); setStatus('disconnected'); }
      } catch (err: any) { addLog('error', `Server switch failed: ${err.message || err}`); setStatus('disconnected'); }
    }
  }, [status, setStatus, activeServer, setActiveServer, addLog, proxyMode, socksPort, httpPort, setConnectedAt]);

  const handleQuickAdd = useCallback(async () => {
    const trimmed = quickInput.trim();
    if (!trimmed) return;
    setQuickImporting(true);
    try {
      if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
        addLog('info', `Fetching subscription: ${trimmed}`);
        const sub = await fetchSubscription(trimmed);
        addSubscription(sub);
        addLog('success', `Loaded ${sub.servers.length} servers from ${sub.name}`);
        setQuickInput('');
      } else if (/^(vless|vmess|trojan|ss|hy2|tuic|wg):\/\//.test(trimmed)) {
        const server = parseProxyLink(trimmed);
        if (server) { addServer(server); addLog('success', `Added server: ${server.name}`); setQuickInput(''); }
        else { addLog('error', 'Invalid proxy link format'); }
      } else { addLog('error', 'Paste a subscription URL (https://...) or proxy link (vless://, vmess://, etc.)'); }
    } catch (err: any) { addLog('error', `Error: ${err.message || err}`); }
    finally { setQuickImporting(false); }
  }, [quickInput, addLog, addSubscription, addServer]);

  const handleQuickPaste = useCallback(async () => {
    try { const text = await navigator.clipboard.readText(); setQuickInput(text); } catch { /* */ }
  }, []);

  const handleTestSubscription = async (sub: any) => {
    setTestingSubId(sub.id);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const toUpdate = servers.filter(s => s.subscriptionId === sub.id);
      addLog('warning', `Testing ${toUpdate.length} servers...`);
      for (const s of toUpdate) {
        if (!s.address) continue;
        setPingingServerId(s.id);
        try { useAppStore.getState().updateServerPing(s.id, await pingServerSmart(s, invoke)); }
        catch { useAppStore.getState().updateServerPing(s.id, -1); }
      }
      addLog('success', 'Ping test complete');
    } catch (err: any) { addLog('error', `Ping test failed: ${err?.message || err}`); }
    finally { setPingingServerId(null); setTestingSubId(null); }
  };

  const handleUpdateSubscription = async (sub: any) => {
    setRefreshingSubId(sub.id);
    try {
      addLog('info', `Updating subscription: ${sub.name}...`);
      const updated = await refreshSubscription(sub);
      updateSubscription(sub.id, updated);
      addLog('success', `Updated ${sub.name}: ${updated.servers.length} servers`);
    } catch (err: any) { addLog('error', `Failed to update ${sub.name}: ${err.message || err}`); }
    finally { setRefreshingSubId(null); }
  };

  const handleTestCustomServers = async () => {
    setTestingSubId('__custom__');
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const custom = servers.filter(s => !s.subscriptionId);
      addLog('warning', `Testing ${custom.length} custom servers...`);
      for (const s of custom) {
        if (!s.address) continue;
        setPingingServerId(s.id);
        try { useAppStore.getState().updateServerPing(s.id, await pingServerSmart(s, invoke)); }
        catch { useAppStore.getState().updateServerPing(s.id, -1); }
      }
      addLog('success', 'Custom servers ping test complete');
    } catch (err: any) { addLog('error', `Custom ping test failed: ${err?.message || err}`); }
    finally { setPingingServerId(null); setTestingSubId(null); }
  };

  const handleRemoveServer = useCallback((serverId: string, serverName: string) => {
    if (!confirm(`Delete custom server "${serverName}"?`)) return;
    if (activeServer?.id === serverId) { handleConnect(); setActiveServer(null); }
    removeServer(serverId);
  }, [activeServer, handleConnect, setActiveServer, removeServer]);

  const handleRemoveAllCustom = useCallback(() => {
    if (!confirm('Delete all custom servers?')) return;
    removeAllManualServers();
    addLog('info', 'Removed all custom servers');
  }, [removeAllManualServers, addLog]);

  const handleRemoveSubscription = useCallback((subId: string) => {
    const sub = subscriptions.find(s => s.id === subId);
    if (!confirm(`Delete subscription "${sub?.name}" and all its servers?`)) return;
    removeSubscription(subId);
  }, [subscriptions, removeSubscription]);

  const canConnect = !!activeServer || servers.length > 0;
  const isConnected = status === 'connected';

  // ═══════════════════════════════════════════════════
  //  Render
  // ═══════════════════════════════════════════════════
  return (
    <div className="flex-1 flex flex-col overflow-hidden animate-fade-in">
      <div className="flex-1 flex flex-col items-center gap-3 px-4 relative overflow-y-auto py-4">
        <RetroBackground />

        {/* + Add button */}
        <button
          onClick={() => { setShowAddModal(!showAddModal); if (!showAddModal) handleQuickPaste(); }}
          disabled={quickImporting}
          className="absolute top-4 right-4 z-30 w-10 h-10 flex items-center justify-center bg-white border-[3px] border-black rounded-xl shadow-[3px_3px_0_#000] cursor-pointer hover:translate-x-[-1px] hover:translate-y-[-1px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none transition-all disabled:opacity-50"
          title={t('addSubOrServer')}
        >
          {quickImporting ? <Loader2 className="w-5 h-5 text-black animate-spin stroke-[3px]" /> : <Plus className="w-5 h-5 text-black stroke-[3px]" />}
        </button>

        {/* Add Modal */}
        {showAddModal && (
          <div className="absolute top-16 right-4 z-40 w-72 bg-white border-[3px] border-black rounded-2xl p-4 shadow-[6px_6px_0_#000] animate-slide-up space-y-3">
            <p className="text-[10px] font-black text-black uppercase tracking-widest">{t('addSubOrServer')}</p>
            <div className="flex gap-2">
              <input type="text" value={quickInput} onChange={(e) => setQuickInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { handleQuickAdd(); setShowAddModal(false); } }}
                autoFocus placeholder={t('pasteHint')}
                className="flex-1 min-w-0 bg-gray-50 border-[2px] border-black rounded-lg px-3 py-2 text-xs text-black placeholder:text-black/30 focus:outline-none font-bold tracking-tight" />
              <button onClick={handleQuickPaste}
                className="w-9 h-9 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer hover:bg-black hover:text-white transition-colors shrink-0">
                <ClipboardPaste className="w-4 h-4 stroke-[2.5px]" />
              </button>
            </div>
            <button onClick={() => { handleQuickAdd(); setShowAddModal(false); }}
              disabled={quickImporting || !quickInput.trim()}
              className="w-full py-2.5 bg-black text-white border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-1 active:shadow-none transition-all disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2">
              {quickImporting ? <><Loader2 className="w-3.5 h-3.5 animate-spin" /> {t('adding')}</> : <><Plus className="w-3.5 h-3.5 stroke-[3px]" /> {t('add')}</>}
            </button>
          </div>
        )}

        {/* ═══ MAIN CONTENT ═══ */}
        {servers.length === 0 && status === 'disconnected' ? (
          <OnboardingCard
            quickInput={quickInput} setQuickInput={setQuickInput}
            onQuickAdd={handleQuickAdd} onQuickPaste={handleQuickPaste}
            importing={quickImporting} t={t}
          />
        ) : (
          <div className="contents">
            <ConnectionControls
              status={status} proxyMode={proxyMode} canConnect={canConnect}
              connectTime={connectTime} onConnect={handleConnect}
              onModeSwitch={handleModeSwitch} t={t}
            />

            {showStats && isConnected && (
              <StatsPanel
                currentDownload={currentDownload} currentUpload={currentUpload}
                totalDown={totalDown} totalUp={totalUp} connectTime={connectTime}
                proxyMode={proxyMode} speedHistory={speedHistory} t={t}
              />
            )}

            <ServerList
              servers={servers} subscriptions={subscriptions} activeServer={activeServer}
              searchQuery={searchQuery} onSearchChange={setSearchQuery}
              collapsedGroups={collapsedGroups}
              onToggleGroup={(id) => setCollapsedGroups(prev => ({ ...prev, [id]: !prev[id] }))}
              onServerSelect={handleServerSelect}
              onTestSubscription={handleTestSubscription}
              onUpdateSubscription={handleUpdateSubscription}
              onRemoveSubscription={handleRemoveSubscription}
              onTestCustomServers={handleTestCustomServers}
              onRemoveAllCustomServers={handleRemoveAllCustom}
              onRemoveServer={handleRemoveServer}
              testingSubId={testingSubId} refreshingSubId={refreshingSubId}
              pingingServerId={pingingServerId} t={t}
            />
          </div>
        )}
      </div>

      <LogsStrip
        logs={logs} showLogs={showLogs}
        onToggleLogs={() => setShowLogs(!showLogs)}
        onClearLogs={clearLogs} logsEndRef={logsEndRef} t={t}
      />
    </div>
  );
}
