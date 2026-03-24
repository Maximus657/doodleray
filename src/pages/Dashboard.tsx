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
  Search,
  Timer,
  Wifi,
  Shield,
  ClipboardPaste,
  Link,
  Loader2,
  Rss,
  RefreshCw,
  Activity,
  Settings as SettingsIcon,
  Trash2,
  Plus,
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
import { formatSpeed, formatTime, protocolLabel, pingServerSmart } from '../lib/utils';
import { refreshSubscription, fetchSubscription } from '../lib/subscription';
import { parseProxyLink } from '../lib/parser';
import { useTranslation } from '../locales';
import { reportConnectionError } from '../lib/workshop-api';

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
    totalDown,
    totalUp,
    addTraffic,
    resetTraffic,
    addSpeedPoint,
    setCurrentSpeed,
    logs,
    addLog,
    clearLogs,
    socksPort,
    httpPort,
    subscriptions,
    updateSubscription,
    removeSubscription,
    autoSelectFastest,
    subAutoUpdateMinutes,
    connectedAt,
    setConnectedAt,
    addSubscription,
    addServer,
    removeServer,
    removeAllManualServers,
    updateServerPing,
    showStats,
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
          } catch {
            updateServerPing(server.id, -1);
          }
          await new Promise(r => setTimeout(r, 30));
        }
      } catch { /* not in tauri env */ }
    })();
    return () => { cancelled = true; };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Compute connection time from stored timestamp (survives page navigation)
  const [connectTime, setConnectTime] = useState(0);
  useEffect(() => {
    if (status !== 'connected' || !connectedAt) {
      setConnectTime(0);
      return;
    }
    // Initialize from stored timestamp
    setConnectTime(Math.floor((Date.now() - connectedAt) / 1000));
    const interval = setInterval(() => {
      setConnectTime(Math.floor((Date.now() - connectedAt) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [status, connectedAt]);

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
            // Report the health drop to server -> Telegram
            const currentServer = useAppStore.getState().activeServer;
            reportConnectionError({
              eventType: 'health_drop',
              serverName: currentServer?.name,
              serverAddress: currentServer?.address,
              serverPort: currentServer?.port,
              protocol: currentServer?.protocol,
              errorMessage: 'SOCKS port not responding (3 consecutive health-check failures)',
            });
            // Auto-reconnect: disconnect then reconnect
            try {
              await invoke('vpn_disconnect');
              setStatus('disconnected');
              healthFailRef.current = 0;
              // Auto-reconnect after brief delay
              setTimeout(() => {
                const btn = document.getElementById('connect-button');
                if (btn) btn.click();
              }, 2000);
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
          
          // Ignore routine connection logs to keep things super friendly and simple
          if (line.match(/tunneling request to tcp|accepted (?:tcp|udp)/)) {
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
        addTraffic(dl, ul);
        addSpeedPoint({ time: formatTime(new Date()), download: dl / 1024, upload: ul / 1024 });
      } catch {
        // Not in tauri env
      }
    }, 1000);
    return () => clearInterval(interval);
  }, [status, addSpeedPoint, setCurrentSpeed, addTraffic]);

  const lastSuggestedRef = useRef<string>('');

  // Point 5: Auto-detect clipboard links
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
             
             if (useAppStore.getState().servers.length > 0) {
               setShowAddModal(true);
             }
             useToastStore.getState().addToast('Clipboard key detected! Ready to connect.', 'success');
             addLog('info', 'Found key in clipboard and pre-filled the input.');
          }
        }
      } catch (e) { /* ignore */ }
    };
    
    window.addEventListener('focus', checkClipboard);
    const timer = setTimeout(checkClipboard, 1500);
    
    return () => {
      window.removeEventListener('focus', checkClipboard);
      clearTimeout(timer);
    };
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

      // If TUN mode, MUST be running as admin — no "continue anyway"
      // Running sing-box.exe elevated while app is not admin causes orphaned processes
      if (proxyMode === 'tun') {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const admin: boolean = await invoke('is_admin');
          if (!admin) {
            addLog('warning', 'TUN mode requires administrator privileges. Restarting...');
            try {
              await invoke('restart_as_admin');
              return; // App will exit and relaunch as admin
            } catch (err: any) {
              addLog('error', 'Could not restart as admin — switching to System Proxy mode');
              const { useToastStore } = await import('../stores/toast-store');
              useToastStore.getState().addToast(
                'Admin required for TUN mode — switched to System Proxy',
                'warning'
              );
              setProxyMode('system-proxy');
              setStatus('disconnected');
              return;
            }
          }
        } catch { /* not in tauri env */ }
      }

      setConnectedAt(null);
      addLog('info', `Starting connection to ${srv.name}...`);

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
            kill_switch: useAppStore.getState().killSwitch,
            // Hysteria2
            obfs_type: srv.obfsType || null,
            obfs_password: srv.obfsPassword || null,
            up_mbps: srv.upMbps || null,
            down_mbps: srv.downMbps || null,
            // TUIC
            congestion_control: srv.congestionControl || null,
            udp_relay_mode: srv.udpRelayMode || null,
            alpn: srv.alpn || null,
            // WireGuard
            private_key: srv.privateKey || null,
            peer_public_key: srv.peerPublicKey || null,
            pre_shared_key: srv.preSharedKey || null,
            local_address: srv.localAddress || null,
            reserved: srv.reserved || null,
            mtu: srv.mtu || null,
            workers: srv.workers || null,
            // Shadowsocks
            encryption: srv.encryption || null,
            // Full raw xray config (DoodleVPN subscriptions)
            raw_xray_config: srv.rawConfig || null,
          }
        });

        if (result.success) {
          addLog('success', result.message);
          // Human friendly logs
          addLog('success', 'Protected & Working ✅');
          setStatus('connected');
          setConnectedAt(Date.now());
        } else {
          // Check if port is busy
          if (result.message.includes('bind') || result.message.includes('10808') || result.message.includes('port')) {
            try {
              const portInfo: any = await invoke('check_port', { port: socksPort });
              if (portInfo.busy) {
                addLog('warning', 'Fixing connection route automatically...');
                await invoke('force_free_port', { port: socksPort });
                await new Promise(r => setTimeout(r, 1000));
                // Retry
                const retry: any = await invoke('vpn_connect', {
                  request: {
                    server_address: srv!.address, server_port: srv!.port, protocol: srv!.protocol, transport: srv!.transport,
                    security: srv!.security, uuid: srv!.uuid, sni: srv!.sni, fingerprint: srv!.fingerprint || 'chrome',
                    public_key: srv!.publicKey, short_id: srv!.shortId, host: srv!.host, path: srv!.path,
                    service_name: (srv as any)?.serviceName || '', proxy_mode: proxyMode, socks_port: socksPort, http_port: httpPort,
                    network_stack: networkStack, dns_mode: dnsMode, strict_route: strictRoute,
                    raw_xray_config: srv!.rawConfig || null,
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
          // Report connection failure to server -> Telegram
          reportConnectionError({
            eventType: 'connect_fail',
            serverName: srv!.name,
            serverAddress: srv!.address,
            serverPort: srv!.port,
            protocol: srv!.protocol,
            errorMessage: result.message,
          });
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
      resetTraffic();
    }
  }, [status, setStatus, setCurrentSpeed, resetTraffic, activeServer, servers, setActiveServer, addLog, proxyMode, socksPort, httpPort, autoSelectFastest, setConnectedAt]);

  const handleModeSwitch = useCallback(async (mode: 'system-proxy' | 'tun') => {
    if (proxyMode === mode) return;
    
    // Switching TO TUN — app MUST be admin, otherwise restart
    if (mode === 'tun') {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const admin: boolean = await invoke('is_admin');
        if (!admin) {
          addLog('warning', 'TUN mode requires admin. Disconnecting and restarting as admin...');
          // Disconnect first, then restart
          if (status === 'connected') {
            await invoke('vpn_disconnect');
          }
          setProxyMode(mode); // Save the preference so after restart it uses TUN
          try {
            await invoke('restart_as_admin');
            return;
          } catch {
            addLog('error', 'Could not restart as admin — staying on System Proxy');
            const { useToastStore } = await import('../stores/toast-store');
            useToastStore.getState().addToast('Admin required for TUN mode', 'warning');
            setProxyMode('system-proxy');
            return;
          }
        }
      } catch { /* not in tauri */ }
    }
    
    setProxyMode(mode);
    addLog('info', `Switched routing mode to ${mode === 'tun' ? 'TUN' : 'System Proxy'}`);
    
    if (status === 'connected') {
      addLog('warning', 'Reconnecting to apply new routing mode...');
      setStatus('connecting');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        // Wait long enough for sing-box.exe to die and ports to be released
        // This is critical when switching from TUN (elevated process) to System Proxy
        await new Promise(r => setTimeout(r, 2000));
        
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
              kill_switch: useAppStore.getState().killSwitch,
              obfs_type: srv.obfsType || null, obfs_password: srv.obfsPassword || null,
              up_mbps: srv.upMbps || null, down_mbps: srv.downMbps || null,
              congestion_control: srv.congestionControl || null,
              udp_relay_mode: srv.udpRelayMode || null, alpn: srv.alpn || null,
              private_key: srv.privateKey || null, peer_public_key: srv.peerPublicKey || null,
              pre_shared_key: srv.preSharedKey || null, local_address: srv.localAddress || null,
              reserved: srv.reserved || null, mtu: srv.mtu || null, workers: srv.workers || null,
              encryption: srv.encryption || null,
              raw_xray_config: srv.rawConfig || null,
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

  // Handle server selection — auto-reconnect if currently connected
  const handleServerSelect = useCallback(async (server: typeof activeServer) => {
    if (!server) return;
    const isSameServer = activeServer?.id === server.id;
    setActiveServer(server);
    setSearchQuery('');
    
    // If connected and selected a DIFFERENT server, auto-reconnect
    if (status === 'connected' && !isSameServer) {
      addLog('warning', `Switching to ${server.name}...`);
      setStatus('connecting');
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        await new Promise(r => setTimeout(r, 2000));
        
        const { networkStack, dnsMode, strictRoute } = useAppStore.getState();
        const myRules = useWorkshopStore.getState().myRules.filter(r => r.enabled);
        const routingRules = myRules.map(r => ({
          rule_type: r.type, value: r.value, action: r.action,
        }));
        const result: any = await invoke('vpn_connect', {
          request: {
            server_address: server.address, server_port: server.port,
            protocol: server.protocol, uuid: server.uuid || null,
            password: server.password || null, transport: server.transport,
            security: server.security, sni: server.sni || null,
            host: server.host || null, path: server.path || null,
            fingerprint: server.fingerprint || null,
            public_key: server.publicKey || null,
            short_id: server.shortId || null, flow: server.flow || null,
            proxy_mode: proxyMode, socks_port: socksPort, http_port: httpPort,
            network_stack: networkStack, dns_mode: dnsMode,
            strict_route: strictRoute, routing_rules: routingRules,
            kill_switch: useAppStore.getState().killSwitch,
            obfs_type: (server as any).obfsType || null, obfs_password: (server as any).obfsPassword || null,
            up_mbps: (server as any).upMbps || null, down_mbps: (server as any).downMbps || null,
            congestion_control: (server as any).congestionControl || null,
            udp_relay_mode: (server as any).udpRelayMode || null, alpn: (server as any).alpn || null,
            private_key: (server as any).privateKey || null, peer_public_key: (server as any).peerPublicKey || null,
            pre_shared_key: (server as any).preSharedKey || null, local_address: (server as any).localAddress || null,
            reserved: (server as any).reserved || null, mtu: (server as any).mtu || null, workers: (server as any).workers || null,
            encryption: (server as any).encryption || null,
            raw_xray_config: (server as any).rawConfig || null,
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
      } catch (err: any) {
        addLog('error', `Server switch failed: ${err.message || err}`);
        setStatus('disconnected');
      }
    }
  }, [status, setStatus, activeServer, setActiveServer, addLog, proxyMode, socksPort, httpPort, setConnectedAt]);

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


  const handleTestSubscription = async (sub: any) => {
    setTestingSubId(sub.id);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const serversToUpdate = servers.filter(s => s.subscriptionId === sub.id);
      addLog('warning', `Testing ${serversToUpdate.length} servers...`);
      for (const s of serversToUpdate) {
        if (!s.address) continue;
        setPingingServerId(s.id);
        try {
          const ping = await pingServerSmart(s, invoke);
          useAppStore.getState().updateServerPing(s.id, ping);
        } catch {
          useAppStore.getState().updateServerPing(s.id, -1);
        }
      }
      addLog('success', 'Ping test complete');
    } catch (err: any) {
      addLog('error', `Ping test failed: ${err?.message || err}`);
    } finally {
      setPingingServerId(null);
      setTestingSubId(null);
    }
  };
  
  const handleUpdateSubscription = async (sub: any) => {
    setRefreshingSubId(sub.id);
    try {
      addLog('info', `Updating subscription: ${sub.name}...`);
      const updated = await refreshSubscription(sub);
      updateSubscription(sub.id, updated);
      addLog('success', `Updated ${sub.name}: ${updated.servers.length} servers`);
    } catch (err: any) {
      addLog('error', `Failed to update ${sub.name}: ${err.message || err}`);
    } finally {
      setRefreshingSubId(null);
    }
  };

  const handleTestCustomServers = async () => {
    setTestingSubId('__custom__');
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const customServers = servers.filter(s => !s.subscriptionId);
      addLog('warning', `Testing ${customServers.length} custom servers...`);
      for (const s of customServers) {
        if (!s.address) continue;
        setPingingServerId(s.id);
        try {
          const ping = await pingServerSmart(s, invoke);
          useAppStore.getState().updateServerPing(s.id, ping);
        } catch {
          useAppStore.getState().updateServerPing(s.id, -1);
        }
      }
      addLog('success', 'Custom servers ping test complete');
    } catch (err: any) {
      addLog('error', `Custom ping test failed: ${err?.message || err}`);
    } finally {
      setPingingServerId(null);
      setTestingSubId(null);
    }
  };

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

        {/* + Add button in top-right corner */}
        <button
          onClick={() => { setShowAddModal(!showAddModal); if (!showAddModal) { handleQuickPaste(); } }}
          disabled={quickImporting}
          className="absolute top-4 right-4 z-30 w-10 h-10 flex items-center justify-center bg-white border-[3px] border-black rounded-xl shadow-[3px_3px_0_#000] cursor-pointer hover:translate-x-[-1px] hover:translate-y-[-1px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none transition-all disabled:opacity-50"
          title="Add server or subscription"
        >
          {quickImporting ? <Loader2 className="w-5 h-5 text-black animate-spin stroke-[3px]" /> : <Plus className="w-5 h-5 text-black stroke-[3px]" />}
        </button>

        {/* Add Modal Popup */}
        {showAddModal && (
          <div className="absolute top-16 right-4 z-40 w-72 bg-white border-[3px] border-black rounded-2xl p-4 shadow-[6px_6px_0_#000] animate-slide-up space-y-3">
            <p className="text-[10px] font-black text-black uppercase tracking-widest">Add subscription or server</p>
            <div className="flex gap-2">
              <input
                type="text"
                value={quickInput}
                onChange={(e) => setQuickInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') { handleQuickAdd(); setShowAddModal(false); } }}
                autoFocus
                placeholder="https://... or vless://..."
                className="flex-1 min-w-0 bg-gray-50 border-[2px] border-black rounded-lg px-3 py-2 text-xs text-black placeholder:text-black/30 focus:outline-none font-bold tracking-tight"
              />
              <button
                onClick={handleQuickPaste}
                className="w-9 h-9 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer hover:bg-black hover:text-white transition-colors shrink-0"
                title="Paste from clipboard"
              >
                <ClipboardPaste className="w-4 h-4 stroke-[2.5px]" />
              </button>
            </div>
            <button
              onClick={() => { handleQuickAdd(); setShowAddModal(false); }}
              disabled={quickImporting || !quickInput.trim()}
              className="w-full py-2.5 bg-black text-white border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-1 active:shadow-none transition-all disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2"
            >
              {quickImporting ? <><Loader2 className="w-3.5 h-3.5 animate-spin" /> Adding...</> : <><Plus className="w-3.5 h-3.5 stroke-[3px]" /> Add</>}
            </button>
          </div>
        )}

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
        <div className="relative flex bg-black rounded-2xl p-1.5 shadow-inner w-full max-w-sm border-[3px] border-black shrink-0 mt-2 z-10">
          {/* Sliding indicator */}
          <div
            className={`absolute top-1.5 bottom-1.5 w-[calc(50%-6px)] bg-bg-primary rounded-xl transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] shadow-[2px_2px_0_rgba(0,0,0,0.4)] border-[2px] border-black ${
              proxyMode === 'tun' ? 'left-1/2' : 'left-1.5'
            }`}
          />
          <button onClick={() => handleModeSwitch('system-proxy')}
            className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2 text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
              ${proxyMode === 'system-proxy' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
            <Globe className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'system-proxy' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('systemProxy')}</span>
          </button>
          <button onClick={() => handleModeSwitch('tun')}
            className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2 text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
              ${proxyMode === 'tun' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
            <Network className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'tun' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('tunMode')}</span>
          </button>
        </div>

        {/* ── POWER BUTTON ── */}
        <div className="flex flex-col items-center mt-2 relative z-10 shrink-0">
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
              <Power className={`w-16 h-16 transition-all duration-300 stroke-[3px] ${isConnecting ? 'animate-pulse' : ''}`} />
            </div>
          </button>
          
          {/* Status Label */}
          {isConnected ? (
            <div className="mt-4 px-6 py-3 bg-black rounded-2xl border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.3)] hover:-translate-y-0.5 hover:shadow-[6px_6px_0_rgba(0,0,0,0.4)] transition-all">
              <p className="text-[13px] font-black tracking-widest uppercase text-emerald-400 text-center flex items-center justify-center gap-2">
                Protected & Working <CheckCircle2 className="w-5 h-5 inline stroke-[3px]" />
              </p>
              <p className="text-[10px] text-emerald-400/50 text-center mt-1 uppercase tracking-widest font-bold">
                Time Valid: {formatDuration(connectTime)}
              </p>
            </div>
          ) : (
            <div className={`mt-4 px-6 py-3 rounded-2xl border-[3px] border-transparent transition-all duration-300 ${isConnecting ? 'bg-amber-100/50 border-amber-400/30' : ''}`}>
              <p className={`text-[13px] font-black tracking-widest uppercase flex items-center justify-center gap-2
                ${isConnecting ? 'text-amber-600 animate-pulse' : 'text-black/40'}`}>
                {isConnecting ? <><Loader2 className="w-4 h-4 inline animate-spin stroke-[3px]" /> Connecting...</> : 'Not Connected ❌'}
              </p>
            </div>
          )}
        </div>

        {/* ── STATS CARDS (when connected) ── */}
        {showStats && isConnected && (
          <div className="grid grid-cols-3 gap-4 w-full max-w-md mt-4 shrink-0 animate-slide-up relative z-10">
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
        {showStats && isConnected && speedHistory.length > 2 && (() => {
          const displayData = speedHistory.slice(-30); // Last 30 seconds
          return (
          <div className="w-full max-w-md card rounded-2xl p-3 shrink-0 animate-slide-up relative z-10 pointer-events-none">
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



        {/* ── INLINE SERVERS LIST ── */}
        <div className="w-full max-w-sm mt-4 relative z-10 pb-4">
          <div className="mb-2 px-1 flex items-center justify-between">
            <span className="text-[11px] font-black text-black/50 uppercase tracking-widest pl-1">{t('activeServer')}</span>
            {servers.length > 5 && (
              <div className="relative w-32">
                <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-black/50 stroke-[3px]" />
                <input type="text" placeholder="Search..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="w-full bg-black/5 rounded-lg pl-7 pr-2 py-1 text-[10px] font-black text-black focus:outline-none placeholder:text-black/30 uppercase tracking-widest focus:bg-white focus:border-black border-[2px] border-transparent" />
              </div>
            )}
          </div>
          
          <div className="flex flex-col gap-4 relative z-20">
            {/* Map over subscriptions to create grouped lists */}
            {subscriptions.map(sub => {
              const subServers = servers.filter(
                s => s.subscriptionId === sub.id && 
                (s.name.toLowerCase().includes(searchQuery.toLowerCase()) || s.countryCode?.toLowerCase() === searchQuery.toLowerCase())
              );
              
              if (subServers.length === 0 && searchQuery) return null; // hide empty groups when searching

              return (
                <div key={sub.id} className="w-full">
                  {/* Subscription Header */}
                  <div className="w-full flex items-center justify-between bg-white border-[3px] border-black rounded-xl p-2.5 mb-2 shadow-[2px_2px_0_#000]">
                    <div className="flex items-center gap-2 min-w-0 pr-2 cursor-pointer select-none" onClick={() => setCollapsedGroups(prev => ({ ...prev, [sub.id]: !prev[sub.id] }))}>
                      <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups[sub.id] ? '-rotate-90' : 'rotate-0'}`} />
                      <Rss className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                      <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">
                        {sub.name}
                      </span>
                      <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">
                        {sub.servers.length}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0 px-1">
                      <button onClick={() => handleUpdateSubscription(sub)}
                        disabled={refreshingSubId === sub.id}
                        className={`w-7 h-7 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer text-black transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none ${refreshingSubId === sub.id ? 'opacity-70 cursor-wait' : ''}`} title="Refresh Subscription">
                        <RefreshCw className={`w-3.5 h-3.5 stroke-[3px] ${refreshingSubId === sub.id ? 'animate-spin' : ''}`} />
                      </button>
                      <button onClick={() => handleTestSubscription(sub)}
                        disabled={testingSubId === sub.id}
                        className={`h-7 px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                          testingSubId === sub.id 
                            ? 'bg-amber-400 animate-pulse text-black cursor-wait' 
                            : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                        }`} title="Test Latency">
                        {testingSubId === sub.id ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                        <span className="text-[10px] font-black tracking-widest uppercase">{testingSubId === sub.id ? 'Testing...' : 'Test'}</span>
                      </button>
                      <button onClick={() => { 
                          if(confirm(`Delete subscription "${sub.name}" and all its servers?`)) {
                            removeSubscription(sub.id);
                          }
                        }}
                        className="w-7 h-7 flex items-center justify-center bg-red-400 border-[2px] border-black rounded-lg text-white cursor-pointer transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none" title="Delete Subscription">
                        <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                      </button>
                    </div>
                  </div>
                  
                  {/* Servers List for this Sub */}
                  {!collapsedGroups[sub.id] && (
                  <div className="flex flex-col gap-2 pl-2 border-l-[3px] border-black/10 ml-2 animate-slide-up">
                    {subServers.map((server) => {
                      const isActive = activeServer?.id === server.id;
                      const pingColor = server.ping && server.ping > 0
                        ? server.ping < 100 ? 'text-emerald-600' : server.ping < 300 ? 'text-amber-600' : 'text-red-600'
                        : server.ping === -1 ? 'text-red-600' : 'text-black/40';
                      
                      return (
                        <button key={server.id} onClick={() => handleServerSelect(server)}
                          className={`w-full p-2.5 rounded-2xl flex items-center gap-3 transition-all duration-150 overflow-hidden relative cursor-pointer
                            ${isActive 
                              ? 'bg-black text-white border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]' 
                              : 'bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
                          <div className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border-[2px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
                            {renderFlag(server.countryCode)}
                          </div>
                          <div className="flex-1 text-left min-w-0 flex items-center justify-between">
                            <div className="min-w-0 pr-2">
                              <p className="text-sm font-black truncate tracking-tight py-0 uppercase leading-tight">{server.name}</p>
                              <p className={`text-[9px] font-black uppercase tracking-widest mt-0.5 ${isActive ? 'text-emerald-400' : 'text-black/50'}`}>
                                {protocolLabel(server.protocol, server.transport)}
                              </p>
                            </div>
                            {server.ping !== undefined && (
                              pingingServerId === server.id ? (
                                <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
                              ) : (
                                <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest ${isActive ? 'text-white/80' : pingColor}`}>
                                  {server.ping === -1 ? 'ERROR' : `${server.ping}ms`}
                                </span>
                              )
                            )}
                            {server.ping === undefined && pingingServerId === server.id && (
                              <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
                            )}
                          </div>
                          {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 shrink-0 ml-1 stroke-[3px]" />}
                        </button>
                      );
                    })}
                  </div>
                  )}
                </div>
              );
            })}

            {/* Standalone Servers (Custom) */}
            {(() => {
              const standalone = servers.filter(
                s => !s.subscriptionId && 
                (s.name.toLowerCase().includes(searchQuery.toLowerCase()) || s.countryCode?.toLowerCase() === searchQuery.toLowerCase())
              );
              
              if (standalone.length === 0) return null;

              return (
                <div className="w-full mt-2">
                  <div className="w-full flex items-center justify-between bg-white border-[3px] border-black rounded-xl p-2.5 mb-2 shadow-[2px_2px_0_#000]">
                    <div className="flex items-center gap-2 min-w-0 pr-2 cursor-pointer select-none" onClick={() => setCollapsedGroups(prev => ({ ...prev, '__custom__': !prev['__custom__'] }))}>
                      <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups['__custom__'] ? '-rotate-90' : 'rotate-0'}`} />
                      <SettingsIcon className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                      <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">
                        Custom Servers
                      </span>
                      <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">
                        {standalone.length}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0 px-1">
                      <button onClick={() => handleTestCustomServers()}
                        disabled={testingSubId === '__custom__'}
                        className={`h-7 px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                          testingSubId === '__custom__' 
                            ? 'bg-amber-400 animate-pulse text-black cursor-wait' 
                            : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                        }`} title="Test Latency">
                        {testingSubId === '__custom__' ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                        <span className="text-[10px] font-black tracking-widest uppercase">{testingSubId === '__custom__' ? 'Testing...' : 'Test'}</span>
                      </button>
                      <button onClick={() => { 
                          if(confirm('Delete all custom servers?')) {
                            removeAllManualServers();
                            addLog('info', 'Removed all custom servers');
                          }
                        }}
                        className="w-7 h-7 flex items-center justify-center bg-red-400 border-[2px] border-black rounded-lg text-white cursor-pointer transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none" title="Delete All Custom Servers">
                        <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                      </button>
                    </div>
                  </div>
                  {!collapsedGroups['__custom__'] && (
                  <div className="flex flex-col gap-2 pl-2 border-l-[3px] border-black/10 ml-2">
                    {standalone.map((server) => {
                      const isActive = activeServer?.id === server.id;
                      const pingColor = server.ping && server.ping > 0
                        ? server.ping < 100 ? 'text-emerald-600' : server.ping < 300 ? 'text-amber-600' : 'text-red-600'
                        : server.ping === -1 ? 'text-red-600' : 'text-black/40';
                      
                      return (
                        <button key={server.id} onClick={() => handleServerSelect(server)}
                          className={`w-full p-2.5 rounded-2xl flex items-center gap-3 transition-all duration-150 overflow-hidden relative cursor-pointer
                            ${isActive 
                              ? 'bg-black text-white border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]' 
                              : 'bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
                          <div className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border-[2px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
                            {renderFlag(server.countryCode)}
                          </div>
                          <div className="flex-1 text-left min-w-0 flex items-center justify-between mr-1">
                            <div className="min-w-0 pr-1 truncate">
                              <p className="text-sm font-black truncate tracking-tight py-0 uppercase leading-tight">{server.name}</p>
                              <p className={`text-[9px] font-black uppercase tracking-widest mt-0.5 ${isActive ? 'text-emerald-400' : 'text-black/50'}`}>
                                {protocolLabel(server.protocol, server.transport)}
                              </p>
                            </div>
                            {server.ping !== undefined && (
                              <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest pl-1 shrink-0 ${isActive ? 'text-white/80' : pingColor}`}>
                                {server.ping === -1 ? 'ERROR' : `${server.ping}ms`}
                              </span>
                            )}
                          </div>
                          
                          <div className="flex items-center gap-1 shrink-0">
                            <button onClick={(e) => {
                                e.stopPropagation();
                                if(confirm(`Delete custom server "${server.name}"?`)) {
                                  if (activeServer?.id === server.id) {
                                    handleConnect(); // disconnect first
                                    setActiveServer(null);
                                  }
                                  removeServer(server.id);
                                }
                              }}
                              className="w-8 h-8 flex items-center justify-center bg-danger/10 hover:bg-danger text-danger hover:text-white rounded-xl transition-colors cursor-pointer"
                              title="Delete server">
                              <Trash2 className="w-4 h-4 stroke-[3px]" />
                            </button>
                            {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 stroke-[3px]" />}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                  )}
                </div>
              );
            })()}
          </div>
        </div>
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
    </div>
  );
}
