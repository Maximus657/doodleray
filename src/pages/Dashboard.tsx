import { useState, useEffect, useCallback, useRef } from 'react';
import {
  Zap,
  Power,
  ArrowDown,
  ArrowUp,
  Globe,
  Network,
  CheckCircle2,
  ScrollText,
  ChevronDown,
  ChevronUp,
  Server as ServerIcon,
  X,
  Search,
  Timer,
  Wifi,
  Shield,
  ClipboardPaste,
  Link,
  Loader2,
} from 'lucide-react';
import {
  AreaChart,
  Area,
  ResponsiveContainer,
  CartesianGrid,
  YAxis,
} from 'recharts';
import { useAppStore } from '../stores/app-store';
import { useWorkshopStore } from '../stores/workshop-store';
import { formatSpeed, formatTime, protocolLabel } from '../lib/utils';
import { refreshSubscription, fetchSubscription } from '../lib/subscription';
import { parseProxyLink } from '../lib/parser';
import { useTranslation } from '../locales';

/* ═══════════════════════════════════════════════════════════ */
/*  Animated Network Mesh Background                          */
/* ═══════════════════════════════════════════════════════════ */
function RetroBackground() {
  return (
    <>
      <div className="absolute inset-0 overflow-hidden pointer-events-none select-none flex items-center justify-center opacity-10">
        {/* Mascot watermark — large centered */}
        <img src="/assets/mascot.png" alt=""
          className="h-[85vh] w-auto drop-shadow-2xl"
          draggable={false} />
      </div>
      {/* Brand name — always visible */}
      <span className="absolute top-4 left-4 text-lg font-black tracking-tight text-black/30 select-none pointer-events-none z-10">
        DOODLERAY
      </span>
    </>
  );
}

export default function Dashboard() {
  const {
    status,
    setStatus,
    activeServer,
    servers,
    setActiveServer,
    proxyMode,
    setProxyMode,
    speedHistory,
    currentDownload,
    currentUpload,
    addSpeedPoint,
    setCurrentSpeed,
    logs,
    addLog,
    clearLogs,
    socksPort,
    httpPort,
    subscriptions,
    updateSubscription,
    autoSelectFastest,
    subAutoUpdateMinutes,
    setConnectedAt,
    addSubscription,
    addServer,
  } = useAppStore();
  const { t } = useTranslation();

  const [connectTime, setConnectTime] = useState(0);
  const [showLogs, setShowLogs] = useState(false);
  const [showServerPicker, setShowServerPicker] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [totalDown, setTotalDown] = useState(0);
  const [totalUp, setTotalUp] = useState(0);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [quickInput, setQuickInput] = useState('');
  const [quickImporting, setQuickImporting] = useState(false);

  // Sync connection state from backend on mount (handles WebView/HMR reloads)
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

  // Connection health monitor — prevents "connected but no internet" 
  const healthFailRef = useRef(0);
  useEffect(() => {
    if (status !== 'connected') {
      healthFailRef.current = 0;
      return;
    }
    const healthCheck = setInterval(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const healthy: boolean = await invoke('check_connection_health', { socksPort: socksPort });
        if (healthy) {
          healthFailRef.current = 0;
        } else {
          healthFailRef.current++;
          if (healthFailRef.current >= 3) {
            addLog('warning', 'Connection lost — SOCKS port not responding. Auto-reconnecting...');
            const { useToastStore } = await import('../stores/toast-store');
            useToastStore.getState().addToast('Connection lost — reconnecting...', 'warning');
            // Auto-reconnect: disconnect then reconnect
            try {
              await invoke('vpn_disconnect');
              setStatus('disconnected');
              healthFailRef.current = 0;
            } catch { /* ignore */ }
          }
        }
      } catch { /* not in tauri env */ }
    }, 30000); // check every 30s
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
          if (!line.trim()) continue;
          // Helper to identify apps by IP or domain
          const identifyTarget = (addr: string) => {
            const lower = addr.toLowerCase();
            if (lower.includes('149.154.') || lower.includes('91.108.') || lower.includes('95.161.')) return `[Telegram] ${addr}`;
            if (lower.includes('discord') || lower.includes('162.159.')) return `[Discord] ${addr}`;
            if (lower.includes('spotify') || lower.includes('scdn.co')) return `[Spotify] ${addr}`;
            if (lower.includes('vkvideo') || lower.includes('vk.com')) return `[VK] ${addr}`;
            if (lower.includes('google') || lower.includes('youtube') || lower.includes('ytimg') || lower.includes('googlevideo')) return `[Google] ${addr}`;
            return addr;
          };

          // Extract useful info from xray log lines
          const tunnelingMatch = line.match(/tunneling request to tcp:([^\s]+)/);
          if (tunnelingMatch) {
            addLog('success', `→ ${identifyTarget(tunnelingMatch[1])}`);
            continue;
          }
          
          const acceptedMatch = line.match(/accepted (?:tcp|udp):([^\s]+) \[([^\]]+)\]/);
          if (acceptedMatch) {
            addLog('info', `⇄ ${identifyTarget(acceptedMatch[1])} [${acceptedMatch[2]}]`);
            continue;
          }
          
          // Show warnings/errors as-is
          if (line.includes('[Warning]') || line.includes('[Error]') || line.includes('Failed')) {
            addLog(line.includes('[Error]') || line.includes('Failed') ? 'error' : 'warning', line);
          }
        }
      } catch (_) { /* not in tauri env or command not available */ }
    }, 1000);
    return () => clearInterval(poll);
  }, [status, addLog]);

  // Get real speed data from xray stats API
  // Poll traffic stats at 1Hz for smooth graph with animation
  useEffect(() => {
    if (status !== 'connected') return;
    const interval = setInterval(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const stats: any = await invoke('get_traffic_stats');
        const dl = stats.download || 0;
        const ul = stats.upload || 0;
        setCurrentSpeed(dl, ul);
        setTotalDown(p => p + dl);
        setTotalUp(p => p + ul);
        addSpeedPoint({ time: formatTime(new Date()), download: dl / 1024, upload: ul / 1024 });
      } catch {
        // Not in tauri env
      }
    }, 1000);
    return () => clearInterval(interval);
  }, [status, addSpeedPoint, setCurrentSpeed]);

  // Timer
  useEffect(() => {
    if (status !== 'connected') { setConnectTime(0); setTotalDown(0); setTotalUp(0); return; }
    const interval = setInterval(() => setConnectTime((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [status]);

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

  const formatDuration = (s: number) => {
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}:${sec.toString().padStart(2, '0')}`;
  };

  const formatTotal = (bytes: number) => {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  };

  const handleConnect = useCallback(async () => {
    if (status === 'disconnected') {
      let srv = activeServer;
      // Only auto-select if user hasn't manually chosen a server
      if (!srv && servers.length > 0) {
        if (autoSelectFastest) {
          const serversWithPing = servers.filter(s => s.ping !== undefined && s.ping > 0);
          if (serversWithPing.length > 0) {
            srv = serversWithPing.reduce((best, s) => (s.ping! < best.ping! ? s : best));
            addLog('info', `Auto-selected fastest: ${srv.name} (${srv.ping}ms)`);
          } else {
            srv = servers[0];
          }
        } else {
          srv = servers[0];
        }
        setActiveServer(srv);
      }
      if (!srv) {
        addLog('error', 'No server selected. Please add a subscription or select a server.');
        return;
      }
      setStatus('connecting');

      // If TUN mode, check if running as admin — offer one-time restart
      if (proxyMode === 'tun') {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const admin: boolean = await invoke('is_admin');
          if (!admin) {
            const confirmRestart = window.confirm(
              'TUN mode requires administrator privileges.\n\n' +
              'Restart DoodleRay as Administrator?\n' +
              '(You\'ll only need to do this once per session)\n\n' +
              'Click Cancel to continue anyway (UAC will appear).'
            );
            if (confirmRestart) {
              addLog('info', 'Restarting as administrator...');
              try {
                await invoke('restart_as_admin');
                return;
              } catch (err: any) {
                addLog('warning', 'Could not restart as admin, continuing with per-toggle UAC');
              }
            }
          }
        } catch { /* not in tauri env */ }
      }

      setConnectedAt(null);
      addLog('info', `Connecting to ${srv.name} (${srv.address}:${srv.port})`);
      addLog('info', `Protocol: ${srv.protocol} | Transport: ${srv.transport} | Security: ${srv.security}`);

      const { networkStack, dnsMode, strictRoute } = useAppStore.getState();
      
      // Get routing rules from Workshop
      const myRules = useWorkshopStore.getState().myRules.filter(r => r.enabled);
      const routingRules = myRules.map(r => ({
        rule_type: r.type,
        value: r.value,
        action: r.action,
      }));
      
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result: any = await invoke('vpn_connect', {
          request: {
            server_address: srv.address,
            server_port: srv.port,
            protocol: srv.protocol,
            uuid: srv.uuid || null,
            password: srv.password || null,
            transport: srv.transport,
            security: srv.security,
            sni: srv.sni || null,
            host: srv.host || null,
            path: srv.path || null,
            fingerprint: srv.fingerprint || null,
            public_key: srv.publicKey || null,
            short_id: srv.shortId || null,
            flow: srv.flow || null,
            proxy_mode: proxyMode,
            socks_port: socksPort,
            http_port: httpPort,
            network_stack: networkStack,
            dns_mode: dnsMode,
            strict_route: strictRoute,
            routing_rules: routingRules,
          }
        });

        if (result.success) {
          addLog('success', result.message);
          if (proxyMode === 'tun') {
            addLog('info', `TUN adapter initialized with ${networkStack} stack`);
          } else {
            addLog('info', `SOCKS5 → 127.0.0.1:${socksPort} | HTTP → 127.0.0.1:${httpPort}`);
          }
          setStatus('connected');
          setConnectedAt(Date.now());
        } else {
          // Check if port is busy
          if (result.message.includes('bind') || result.message.includes('10808') || result.message.includes('port')) {
            try {
              const portInfo: any = await invoke('check_port', { port: socksPort });
              if (portInfo.busy) {
                addLog('error', `⚠ Port ${socksPort} is busy: ${portInfo.process} (PID ${portInfo.pid})`);
                addLog('warning', 'Attempting to free the port...');
                await invoke('force_free_port', { port: socksPort });
                addLog('info', 'Port freed! Retrying connection in 1 second...');
                await new Promise(r => setTimeout(r, 1000));
                // Retry
                const retry: any = await invoke('vpn_connect', {
                  request: {
                    server_address: srv!.address, server_port: srv!.port, protocol: srv!.protocol, transport: srv!.transport,
                    security: srv!.security, uuid: srv!.uuid, sni: srv!.sni, fingerprint: srv!.fingerprint || 'chrome',
                    public_key: srv!.publicKey, short_id: srv!.shortId, host: srv!.host, path: srv!.path,
                    service_name: (srv as any)?.serviceName || '', proxy_mode: proxyMode, socks_port: socksPort, http_port: httpPort,
                    network_stack: networkStack, dns_mode: dnsMode, strict_route: strictRoute,
                  }
                });
                if (retry.success) {
                  addLog('success', retry.message);
                  setStatus('connected');
                  setConnectedAt(Date.now());
                  return;
                }
              }
            } catch {}
          }
          addLog('error', result.message);
          setStatus('disconnected');
        }
      } catch (err: any) {
        // Tauri not available (browser dev mode) — simulation
        addLog('warning', `Dev mode — simulating connection: ${err.message || err}`);
        setTimeout(() => {
          addLog('success', `[SIM] Connected via ${srv!.protocol.toUpperCase()}+${srv!.transport}`);
          setStatus('connected');
          setConnectedAt(Date.now());
        }, 1500);
      }
    } else if (status === 'connected') {
      addLog('warning', 'Disconnecting...');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result: any = await invoke('vpn_disconnect');
        if (result.success) {
          addLog('info', result.message);
        } else {
          addLog('error', result.message);
        }
      } catch {
        addLog('info', '[SIM] Disconnected');
      }
      setStatus('disconnected');
      setCurrentSpeed(0, 0);
    }
  }, [status, setStatus, setCurrentSpeed, activeServer, servers, setActiveServer, addLog, proxyMode, socksPort, httpPort, autoSelectFastest, setConnectedAt]);

  const handleModeSwitch = useCallback(async (mode: 'system-proxy' | 'tun') => {
    if (proxyMode === mode) return;
    setProxyMode(mode);
    addLog('info', `Switched routing mode to ${mode === 'tun' ? 'TUN' : 'System Proxy'}`);
    
    if (status === 'connected') {
      addLog('warning', 'Reconnecting to apply new routing mode...');
      setStatus('connecting');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        await new Promise(r => setTimeout(r, 500));
        
        // Reconnect directly via invoke instead of fragile btn.click()
        const srv = activeServer;
        if (srv) {
          const { networkStack, dnsMode, strictRoute } = useAppStore.getState();
          const myRules = useWorkshopStore.getState().myRules.filter(r => r.enabled);
          const routingRules = myRules.map(r => ({
            rule_type: r.type, value: r.value, action: r.action,
          }));
          const result: any = await invoke('vpn_connect', {
            request: {
              server_address: srv.address, server_port: srv.port,
              protocol: srv.protocol, uuid: srv.uuid || null,
              password: srv.password || null, transport: srv.transport,
              security: srv.security, sni: srv.sni || null,
              host: srv.host || null, path: srv.path || null,
              fingerprint: srv.fingerprint || null,
              public_key: srv.publicKey || null,
              short_id: srv.shortId || null, flow: srv.flow || null,
              proxy_mode: mode, socks_port: socksPort, http_port: httpPort,
              network_stack: networkStack, dns_mode: dnsMode,
              strict_route: strictRoute, routing_rules: routingRules,
            }
          });
          if (result.success) {
            addLog('success', result.message);
            setStatus('connected');
            setConnectedAt(Date.now());
          } else {
            addLog('error', result.message);
            setStatus('disconnected');
          }
        } else {
          setStatus('disconnected');
        }
      } catch (err: any) {
        addLog('error', `Reconnect failed: ${err.message || err}`);
        setStatus('disconnected');
      }
    }
  }, [proxyMode, setProxyMode, status, setStatus, addLog, activeServer, socksPort, httpPort, setConnectedAt]);

  const canConnect = activeServer || servers.length > 0;

  // Quick add handler for Dashboard input
  const handleQuickAdd = useCallback(async () => {
    const trimmed = quickInput.trim();
    if (!trimmed) return;
    setQuickImporting(true);
    try {
      if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
        // Subscription URL
        addLog('info', `Fetching subscription: ${trimmed}`);
        const sub = await fetchSubscription(trimmed);
        addSubscription(sub);
        addLog('success', `Loaded ${sub.servers.length} servers from ${sub.name}`);
        setQuickInput('');
      } else if (/^(vless|vmess|trojan|ss|hy2|tuic|wg):\/\//.test(trimmed)) {
        // Single proxy link
        const server = parseProxyLink(trimmed);
        if (server) {
          addServer(server);
          addLog('success', `Added server: ${server.name}`);
          setQuickInput('');
        } else {
          addLog('error', 'Invalid proxy link format');
        }
      } else {
        addLog('error', 'Paste a subscription URL (https://...) or proxy link (vless://, vmess://, etc.)');
      }
    } catch (err: any) {
      addLog('error', `Error: ${err.message || err}`);
    } finally {
      setQuickImporting(false);
    }
  }, [quickInput, addLog, addSubscription, addServer]);

  const handleQuickPaste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      setQuickInput(text);
    } catch { /* */ }
  }, []);

  const renderFlag = (code?: string) => {
    if (!code || code.length !== 2) return <Globe className="w-4 h-4 text-text-on-orange-muted" />;
    return <img src={`https://flagcdn.com/w40/${code.toLowerCase()}.png`} alt={code} className="w-6 h-4 object-cover rounded-sm shadow-sm" />;
  };

  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';

  return (
    <div className="flex-1 flex flex-col overflow-hidden animate-fade-in">
      {/* MAIN AREA — scrollable */}
      <div className="flex-1 flex flex-col items-center gap-3 px-4 relative overflow-y-auto py-4">
        
        {/* Retro background */}
        <RetroBackground />

        {/* ══════════════════════════════════════════════ */}
        {/*  ONBOARDING: show only when zero servers      */}
        {/* ══════════════════════════════════════════════ */}
        {servers.length === 0 && status === 'disconnected' ? (
          <div className="flex-1 flex flex-col items-center justify-center w-full max-w-md relative z-10 animate-slide-up gap-6 py-10">
            {/* Logo */}
            <div className="flex items-center gap-3 mb-2">
              <div className="w-14 h-14 bg-black rounded-2xl flex items-center justify-center border-[3px] border-black shadow-[4px_4px_0_#000]">
                <Zap className="w-7 h-7 text-white stroke-[3px]" />
              </div>
              <div>
                <h1 className="text-3xl font-black text-black tracking-tighter uppercase leading-none">DoodleRay</h1>
                <p className="text-[10px] font-black text-black/40 uppercase tracking-widest">Fast & Secure VPN</p>
              </div>
            </div>

            {/* Big onboarding card */}
            <div className="w-full bg-white border-[4px] border-black rounded-3xl p-8 shadow-[8px_8px_0_#000] space-y-5">
              <div className="text-center space-y-2">
                <h2 className="text-xl font-black text-black uppercase tracking-tight">Welcome!</h2>
                <p className="text-xs font-bold text-black/50 uppercase tracking-widest">
                  Paste a subscription URL or proxy link to get started
                </p>
              </div>

              <div className="flex gap-2">
                <input
                  type="text"
                  value={quickInput}
                  onChange={(e) => setQuickInput(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleQuickAdd()}
                  autoFocus
                  placeholder="https://... or vless://..."
                  className="flex-1 bg-gray-50 border-[3px] border-black rounded-xl px-4 py-4 text-sm text-black placeholder:text-black/25 focus:outline-none focus:shadow-[2px_2px_0_#000] font-bold tracking-tight transition-shadow"
                />
                <button
                  onClick={handleQuickPaste}
                  className="group p-4 bg-white border-[3px] border-black rounded-xl shadow-[2px_2px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none cursor-pointer transition-all hover:bg-gray-50"
                  title="Paste from clipboard"
                >
                  <ClipboardPaste className="w-5 h-5 text-black stroke-[2.5px] transition-transform duration-300 group-hover:scale-110 group-hover:-rotate-6" />
                </button>
              </div>

              <button
                onClick={handleQuickAdd}
                disabled={quickImporting || !quickInput.trim()}
                className="w-full py-4 bg-black text-white border-[3px] border-black rounded-2xl text-sm font-black uppercase tracking-widest cursor-pointer shadow-[6px_6px_0_#000] hover:-translate-y-1 hover:-translate-x-1 hover:shadow-[8px_8px_0_#000] active:translate-x-[4px] active:translate-y-[4px] active:shadow-none transition-all disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2"
              >
                {quickImporting ? (
                  <><Loader2 className="w-5 h-5 animate-spin" /> Loading...</>
                ) : (
                  <><Link className="w-5 h-5 stroke-[3px]" /> Connect</>
                )}
              </button>
            </div>
          </div>
        ) : (
        <div className="contents">
        {/* ── PROXY MODE TOGGLE ── */}
        <div className="relative flex bg-black rounded-2xl p-1.5 shadow-inner z-10 w-full max-w-[340px] border-[3px] border-black">
          {/* Sliding indicator */}
          <div
            className={`absolute top-1.5 bottom-1.5 w-[calc(50%-6px)] bg-bg-primary rounded-xl transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] shadow-[2px_2px_0_rgba(0,0,0,0.4)] border-[2px] border-black ${
              proxyMode === 'tun' ? 'left-1/2' : 'left-1.5'
            }`}
          />
          <button onClick={() => handleModeSwitch('system-proxy')}
            className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2.5 rounded-xl text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
              ${proxyMode === 'system-proxy' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
            <Globe className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'system-proxy' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('systemProxy')}</span>
          </button>
          <button onClick={() => handleModeSwitch('tun')}
            className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2.5 rounded-xl text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
              ${proxyMode === 'tun' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
            <Network className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'tun' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('tunMode')}</span>
          </button>
        </div>

        {/* ── SERVER SELECTOR ── */}
        <div className="w-full max-w-sm mt-4 relative z-10">
          <button onClick={() => setShowServerPicker(true)}
            className="w-full bg-white border-[4px] border-black rounded-3xl p-4 flex items-center gap-4 cursor-pointer hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0px_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-[2px_2px_0px_#000] transition-all shadow-[4px_4px_0px_#000] group">
            <div className="w-12 h-12 rounded-2xl bg-black flex items-center justify-center shrink-0 border-2 border-black">
              {activeServer ? renderFlag(activeServer.countryCode) : <ServerIcon className="w-6 h-6 text-white" />}
            </div>
            <div className="flex-1 text-left min-w-0">
              <p className="text-[11px] font-black text-black/60 uppercase tracking-widest mb-0.5">
                {activeServer ? t('activeServer') : t('noServerSelected')}
              </p>
              {activeServer ? (
                <>
                  <p className="text-xl font-black text-black truncate tracking-tight uppercase leading-none mt-1">{activeServer.name}</p>
                  <p className="text-[10px] font-black text-bg-primary uppercase mt-1.5 tracking-wide">
                    {protocolLabel(activeServer.protocol, activeServer.transport)}
                  </p>
                </>
              ) : (
                <p className="text-sm font-black text-black uppercase">{t('selectServerHint')}</p>
              )}
            </div>
            <ChevronDown className="w-6 h-6 text-black transition-transform duration-300 group-hover:scale-110 group-hover:-translate-y-1 stroke-[3px]" />
          </button>
        </div>

        {/* ── POWER BUTTON ── */}
        <div className="flex flex-col items-center mt-6 relative z-10">
          <button id="connect-button" onClick={handleConnect}
            disabled={isConnecting || !canConnect}
            className="group relative w-40 h-40 flex items-center justify-center transition-all duration-150 cursor-pointer border-0 bg-transparent disabled:cursor-not-allowed">
            
            {/* Connecting spinner ring */}
            {isConnecting && (
              <svg className="absolute inset-[-12px] w-[calc(100%+24px)] h-[calc(100%+24px)] animate-spin-slow" viewBox="0 0 100 100">
                <circle cx="50" cy="50" r="46" fill="none" stroke="#000" strokeWidth="4" strokeDasharray="40 240" strokeLinecap="round" />
              </svg>
            )}

            {/* Main Button Body - Brutalist */}
            <div className={`relative w-40 h-40 rounded-full flex items-center justify-center transition-all duration-150 border-[5px] border-black
              ${isConnected
                ? 'bg-black text-white shadow-[0_0_0_#000] scale-95'
                : isConnecting
                  ? 'bg-white text-black shadow-[4px_4px_0_#000]'
                  : canConnect
                    ? 'bg-white text-black shadow-[8px_8px_0_#000] hover:shadow-[10px_10px_0_#000] hover:-translate-y-1 hover:-translate-x-1 active:shadow-[2px_2px_0_#000] active:translate-y-[6px] active:translate-x-[6px]'
                    : 'bg-white/50 text-black/30 shadow-[4px_4px_0_rgba(0,0,0,0.3)]'
              }`}>
              <Power className={`w-16 h-16 transition-all duration-300 stroke-[3px] ${isConnecting ? 'animate-pulse' : 'group-hover:scale-110 group-hover:rotate-[15deg]'}`} />
            </div>
          </button>
          
          {/* Status Label */}
          <p className={`text-sm font-black mt-3 tracking-widest uppercase transition-all duration-300
            ${isConnected ? 'text-emerald-700' : isConnecting ? 'text-amber-600' : 'text-text-on-orange-muted/60'}`}>
            {isConnected ? `${t('connected')} · ${formatDuration(connectTime)}` : isConnecting ? t('connecting') : t('connect')}
          </p>
        </div>

        {/* ── STATS CARDS (when connected) ── */}
        {isConnected && (
          <div className="grid grid-cols-3 gap-4 w-full max-w-md mt-6 animate-slide-up relative z-10">
            {/* Download */}
            <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
              <ArrowDown className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
              <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatSpeed(currentDownload)}</p>
              <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">{t('download')}</p>
              <p className="text-[10px] font-black font-mono text-black/40 mt-1">{formatTotal(totalDown)}</p>
            </div>
            {/* Upload */}
            <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
              <ArrowUp className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
              <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatSpeed(currentUpload)}</p>
              <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">{t('upload')}</p>
              <p className="text-[10px] font-black font-mono text-black/40 mt-1">{formatTotal(totalUp)}</p>
            </div>
            {/* Session Info */}
            <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
              <Timer className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
              <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatDuration(connectTime)}</p>
              <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">Time</p>
              <p className="text-[10px] font-black font-mono text-black/40 mt-1 flex items-center justify-center gap-1">
                <Shield className="w-3 h-3" /> {proxyMode === 'tun' ? 'TUN' : 'PROXY'}
              </p>
            </div>
          </div>
        )}

        {/* ── SPEED GRAPH (when connected) ── */}
        {isConnected && speedHistory.length > 2 && (() => {
          const displayData = speedHistory.slice(-30); // Last 30 seconds
          return (
          <div className="w-full max-w-md card rounded-2xl p-3 animate-slide-up relative z-10 pointer-events-none">
            <div className="flex items-center justify-between mb-2 px-1">
              <span className="text-[10px] font-bold text-text-on-dark-muted uppercase tracking-widest flex items-center gap-1">
                <Wifi className="w-3 h-3" /> Live Throughput
              </span>
              <div className="flex items-center gap-3">
                <span className="text-[9px] text-emerald-400 font-bold flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-emerald-400 inline-block" /> DL</span>
                <span className="text-[9px] text-blue-400 font-bold flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-blue-400 inline-block" /> UL</span>
              </div>
            </div>
            <div className="h-24 w-full">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={displayData} margin={{ top: 2, right: 5, left: 0, bottom: 0 }}>
                  <defs>
                    <linearGradient id="dlGrad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#34d399" stopOpacity={0.5} />
                      <stop offset="100%" stopColor="#34d399" stopOpacity={0.05} />
                    </linearGradient>
                    <linearGradient id="ulGrad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#60a5fa" stopOpacity={0.4} />
                      <stop offset="100%" stopColor="#60a5fa" stopOpacity={0.05} />
                    </linearGradient>
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
                  <YAxis hide domain={[0, 'auto']} />
                  <Area type="monotone" dataKey="download" stroke="#34d399" fill="url(#dlGrad)" strokeWidth={2} dot={false} isAnimationActive={true} animationDuration={900} animationEasing="ease-in-out" connectNulls />
                  <Area type="monotone" dataKey="upload" stroke="#60a5fa" fill="url(#ulGrad)" strokeWidth={1.5} dot={false} isAnimationActive={true} animationDuration={900} animationEasing="ease-in-out" connectNulls />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>
          );
        })()}
        </div>
        )}
      </div>

      {/* ── LOGS STRIP ── */}
      <div className={`bg-white border-t-[4px] border-black overflow-hidden flex flex-col transition-all duration-300 shrink-0
        ${showLogs ? 'h-40' : 'h-10'}`}>
        <button onClick={() => setShowLogs(!showLogs)}
          className="flex items-center justify-between px-4 py-2 shrink-0 cursor-pointer hover:bg-black/5 transition-all">
          <div className="flex items-center gap-2 text-black font-black uppercase tracking-widest text-[11px]">
            <ScrollText className="w-4 h-4" />
            <span>Terminal</span>
            {logs.length > 0 && <span className="text-black/50">({logs.length})</span>}
          </div>
          <div className="flex items-center gap-3">
            {logs.length > 0 && (
              <button onClick={(e) => { e.stopPropagation(); clearLogs(); }} className="text-[10px] uppercase font-black text-black/50 hover:text-black cursor-pointer">Clear</button>
            )}
            {showLogs ? <ChevronDown className="w-5 h-5 text-black stroke-[3px]" /> : <ChevronUp className="w-5 h-5 text-black stroke-[3px]" />}
          </div>
        </button>
        <div className="flex-1 overflow-y-auto px-4 pb-2 font-mono text-[11px] font-black uppercase space-y-1">
          {logs.length === 0 ? (
            <p className="text-black/40 py-2 text-center text-[10px]">No logs yet</p>
          ) : (
            logs.map((log) => (
              <div key={log.id} className="flex gap-3 border-b border-black/5 pb-1">
                <span className="text-black/40 shrink-0 whitespace-nowrap">{log.time}</span>
                <span className={`break-words ${
                  log.level === 'error' ? 'text-red-600' :
                  log.level === 'warning' ? 'text-orange-600' :
                  log.level === 'success' ? 'text-emerald-700' :
                  'text-black'
                }`}>{log.message}</span>
              </div>
            ))
          )}
          <div ref={logsEndRef} />
        </div>
      </div>

      {/* ── SERVER PICKER MODAL ── */}
      {showServerPicker && (
        <div className="absolute inset-0 z-50 flex flex-col bg-bg-primary">
          <div className="flex items-center justify-between p-6 border-b-[4px] border-black bg-white">
            <h2 className="text-2xl font-black text-black tracking-tighter uppercase">Select Server</h2>
            <button onClick={() => setShowServerPicker(false)}
              className="w-10 h-10 bg-black hover:bg-black/80 rounded-xl flex items-center justify-center text-white border-[3px] border-black shadow-[2px_2px_0_#000] cursor-pointer active:translate-x-1 active:translate-y-1 active:shadow-none">
              <X className="w-6 h-6 stroke-[3px]" />
            </button>
          </div>
          <div className="px-6 py-6 flex-1 flex flex-col min-h-0">
            <div className="relative w-full mb-6 shrink-0">
              <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-6 h-6 text-black stroke-[3px]" />
              <input type="text" placeholder="Search servers..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="w-full bg-white border-[4px] border-black shadow-[4px_4px_0_#000] rounded-2xl pl-12 pr-4 py-4 text-sm font-black text-black focus:outline-none focus:translate-x-[-2px] focus:translate-y-[-2px] focus:shadow-[6px_6px_0_#000] placeholder:text-black/40 uppercase" />
            </div>
            <div className="flex-1 overflow-y-auto pr-2 space-y-4">
              {servers.length === 0 ? (
                <div className="text-center py-12 bg-white border-[4px] border-black shadow-[4px_4px_0_#000] rounded-3xl">
                  <p className="text-black font-black uppercase text-xl">No servers available.</p>
                  <p className="text-xs text-black/60 mt-2 font-black uppercase">Go to the Servers tab to add a link.</p>
                </div>
              ) : (
                servers
                  .filter(s => s.name.toLowerCase().includes(searchQuery.toLowerCase()) || s.countryCode?.toLowerCase() === searchQuery.toLowerCase())
                  .map((server) => {
                    const isActive = activeServer?.id === server.id;
                    const pingColor = server.ping
                      ? server.ping < 100 ? 'text-emerald-700' : server.ping < 300 ? 'text-amber-700' : 'text-red-700'
                      : 'text-black/50';
                    return (
                      <button key={server.id} onClick={() => { setActiveServer(server); setShowServerPicker(false); setSearchQuery(''); }}
                        className={`w-full p-4 rounded-3xl flex items-center gap-4 transition-all duration-150 overflow-hidden relative cursor-pointer
                          ${isActive ? 'bg-black text-white border-[4px] border-black scale-[1.02] shadow-[6px_6px_0_rgba(0,0,0,0.4)]' : 'bg-white text-black border-[4px] border-black shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-[2px_2px_0_#000]'}`}>
                        <div className={`w-12 h-12 rounded-xl flex items-center justify-center shrink-0 border-[3px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
                          {renderFlag(server.countryCode)}
                        </div>
                        <div className="flex-1 text-left min-w-0">
                          <p className="text-lg font-black truncate tracking-tighter uppercase leading-none">{server.name}</p>
                          <p className={`text-[10px] font-black uppercase tracking-widest mt-2 ${isActive ? 'text-white/60' : 'text-black/50'}`}>
                            {protocolLabel(server.protocol, server.transport)}
                          </p>
                        </div>
                        {server.ping && (
                          <span className={`text-sm font-black uppercase tracking-widest ${isActive ? 'text-white' : pingColor}`}>
                            {server.ping}ms
                          </span>
                        )}
                        {isActive && <CheckCircle2 className="w-6 h-6 text-white shrink-0 ml-2 stroke-[3px]" />}
                      </button>
                    );
                  })
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
