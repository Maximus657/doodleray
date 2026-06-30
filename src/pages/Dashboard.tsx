import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Plus, Loader2, ClipboardPaste } from 'lucide-react';
import { useAppStore } from '../stores/app-store';
import { formatTime } from '../lib/utils';
import { refreshSubscription, fetchSubscription } from '../lib/subscription';
import { parseProxyLink } from '../lib/parser';
import { useTranslation } from '../locales';
import { reportConnectionError } from '../lib/workshop-api';
import { buildConnectRequestFromState, getActiveRoutingRules } from '../lib/connect-helpers';
import {
  extractPortsFromHealth,
  isHealthAcceptable,
  summarizeHealthFailures,
  waitForConnectionHealth,
  type ConnectionHealthReport,
} from '../lib/connection-health';
import { buildServerSelectionIndex, findMatchingServer, findMatchingServerInIndex, resolveConnectServer } from '../lib/server-selection';
import { getSubscriptionById, getSubscriptionTrafficStatus } from '../lib/subscription-status';
import { pingServersWithLimit } from '../lib/ping-runner';

// Sub-components
import RetroBackground from '../components/dashboard/RetroBackground';
import OnboardingCard from '../components/dashboard/OnboardingCard';
import ConnectionControls from '../components/dashboard/ConnectionControls';
import DashboardControlsDrawer from '../components/dashboard/DashboardControlsDrawer';
import ServerList from '../components/dashboard/ServerList';
import LogsStrip from '../components/dashboard/LogsStrip';

const TRAFFIC_LIMIT_EOF_WINDOW_MS = 12_000;
const TRAFFIC_LIMIT_EOF_THRESHOLD = 4;
const TRAFFIC_LIMIT_NOTICE_COOLDOWN_MS = 60_000;
const CONNECT_TIMEOUT_MS = 45_000;

function isProxyResponseEofLine(line: string): boolean {
  const lower = line.toLowerCase();
  return (
    lower.includes('proxy/http') &&
    lower.includes('failed to read response') &&
    (lower.includes('unexpected eof') || lower.includes('eof'))
  );
}

function isBenignProxyHttpRequestResetLine(line: string): boolean {
  const lower = line.toLowerCase();
  const isLoopbackHttpInbound =
    /127\.0\.0\.1:\d+\s*->\s*127\.0\.0\.1:\d+/.test(line) ||
    /\[::1\]:\d+\s*->\s*\[::1\]:\d+/.test(line);

  return (
    lower.includes('proxy/http') &&
    lower.includes('failed to read http request') &&
    isLoopbackHttpInbound &&
    (
      lower.includes('forcibly closed by the remote host') ||
      lower.includes('connection reset by peer') ||
      lower.includes('wsarecv') ||
      lower.includes('wsasend')
    )
  );
}

function isBenignProxyTeardownLine(line: string): boolean {
  if (isBenignProxyHttpRequestResetLine(line)) return true;

  const lower = line.toLowerCase();
  const hasProxyContext =
    lower.includes('app/proxyman') ||
    lower.includes('proxy/') ||
    lower.includes('transport/internet') ||
    lower.includes('connection ends');
  const hasTeardownReason =
    lower.includes('context canceled') ||
    lower.includes('operation was canceled') ||
    lower.includes('use of closed network connection') ||
    lower.includes('forcibly closed') ||
    lower.includes('connection reset by peer') ||
    lower.includes('broken pipe') ||
    lower.includes('wsarecv') ||
    lower.includes('wsasend') ||
    lower.includes('aborted by the software');
  const isStartupOrReadinessError =
    lower.includes('failed to start') ||
    lower.includes('bind:') ||
    lower.includes('address already in use') ||
    lower.includes('not ready') ||
    lower.includes('initialize');

  return hasProxyContext && hasTeardownReason && !isStartupOrReadinessError;
}

function getProxyLogLevel(line: string): 'error' | 'warning' | null {
  const lower = line.toLowerCase();
  if (lower.includes('[warning]') && lower.includes('core: xray') && lower.includes('started')) return null;
  if (lower.includes('[error]') || lower.includes('failed')) return 'error';
  if (lower.includes('[warning]') || lower.includes('warning')) return 'warning';
  return null;
}

function subscriptionErrorEventType(message: string) {
  return message.toLowerCase().includes('private, loopback, or link-local')
    ? 'dns_private_ip' as const
    : 'subscription_fetch_fail' as const;
}

function formatMessage(template: string, values: Record<string, string | number>) {
  return Object.entries(values).reduce(
    (message, [key, value]) => message.replace(new RegExp(`\\{${key}\\}`, 'g'), String(value)),
    template
  );
}

function isTauriRuntime() {
  const tauriInternals = (window as unknown as {
    __TAURI_INTERNALS__?: { invoke?: unknown };
  }).__TAURI_INTERNALS__;
  return typeof tauriInternals?.invoke === 'function';
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(message)), timeoutMs);
  });

  return Promise.race([promise, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

function extractLocalProxyPorts(message: string): { socksPort: number; httpPort: number } | null {
  const match = message.match(/SOCKS5:\s*127\.0\.0\.1:(\d+),\s*HTTP:\s*127\.0\.0\.1:(\d+)/i);
  if (!match) return null;
  const parsedSocks = Number(match[1]);
  const parsedHttp = Number(match[2]);
  if (!Number.isInteger(parsedSocks) || !Number.isInteger(parsedHttp)) return null;
  if (parsedSocks <= 0 || parsedSocks > 65535 || parsedHttp <= 0 || parsedHttp > 65535) return null;
  return { socksPort: parsedSocks, httpPort: parsedHttp };
}

export default function Dashboard() {
  const {
    status, setStatus, activeServer, servers, setActiveServer,
    proxyMode, setProxyMode, systemProxyMode, setSystemProxyMode, speedHistory, currentDownload, currentUpload,
    totalDown, totalUp, addTraffic, resetTraffic, addSpeedPoint, setCurrentSpeed,
    logs, addLog, clearLogs, socksPort, httpPort, subscriptions,
    updateSubscription, removeSubscription, autoSelectFastest,
    subAutoUpdateMinutes, connectedAt, setConnectedAt,
    addSubscription, addServer, removeServer, removeAllManualServers,
    updateServerPings, showStats, setSocksPort, setHttpPort,
  } = useAppStore();
  const { t } = useTranslation();

  const [showLogs, setShowLogs] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [quickInput, setQuickInput] = useState('');
  const [quickImporting, setQuickImporting] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [connectionStep, setConnectionStep] = useState<string | null>(null);

  const refreshTunnelServiceHealth = useCallback(async () => {
    if (!isTauriRuntime()) return false;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('tunnel_service_health');
      return true;
    } catch {
      return false;
    }
  }, []);

  const markConnectedIfHealthy = useCallback(async (
    result: any,
    invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>,
    mode: 'system-proxy' | 'tun',
    nextSystemProxyMode: typeof systemProxyMode,
    fallbackSocksPort: number,
    fallbackHttpPort: number,
  ) => {
    const actualPorts = extractLocalProxyPorts(result.message || '');
    let effectiveSocksPort = actualPorts?.socksPort ?? fallbackSocksPort;
    let effectiveHttpPort = actualPorts?.httpPort ?? fallbackHttpPort;
    if (actualPorts) {
      setSocksPort(actualPorts.socksPort);
      setHttpPort(actualPorts.httpPort);
    }

    let { health, socksPort: waitedSocksPort, httpPort: waitedHttpPort } = await waitForConnectionHealth(
      invoke,
      mode,
      nextSystemProxyMode,
      effectiveSocksPort,
      effectiveHttpPort,
      (result.health ?? null) as ConnectionHealthReport | null,
    );
    effectiveSocksPort = waitedSocksPort;
    effectiveHttpPort = waitedHttpPort;
    setSocksPort(effectiveSocksPort);
    setHttpPort(effectiveHttpPort);

    if (!isHealthAcceptable(mode, health) && mode === 'tun') {
      addLog('warning', `Protected mode health is ${health?.verdict ?? 'missing'}; running automatic repair once...`);
      try {
        const repairMessage = await invoke('repair_windows_runtime') as string;
        addLog('info', repairMessage.split('\n').slice(0, 3).join(' | '));
      } catch (repairErr: any) {
        addLog('warning', `Automatic repair did not complete: ${repairErr?.message || repairErr}`);
      }
      const repaired = await waitForConnectionHealth(
        invoke,
        mode,
        nextSystemProxyMode,
        effectiveSocksPort,
        effectiveHttpPort,
        health,
        4,
        1500,
      );
      health = repaired.health;
      effectiveSocksPort = repaired.socksPort;
      effectiveHttpPort = repaired.httpPort;
      setSocksPort(effectiveSocksPort);
      setHttpPort(effectiveHttpPort);
    }

    if (!isHealthAcceptable(mode, health)) {
      addLog('error', `Connection started but health quorum failed: ${summarizeHealthFailures(health)}`);
      try { await invoke('vpn_disconnect'); } catch { /* best effort cleanup */ }
      setStatus('disconnected');
      setConnectionStep(null);
      setConnectedAt(null);
      return false;
    }

    if (mode === 'tun' && health?.verdict === 'protected_degraded') {
      addLog('warning', `Весь компьютер подключен, совместимость браузеров восстанавливается: ${summarizeHealthFailures(health)}`);
    }
    addLog('success', result.message);
    addLog('success', t('connectionActive'));
    setConnectionStep(t('connectionReady'));
    setStatus('connected');
    setConnectedAt(Date.now());
    return true;
  }, [addLog, setConnectedAt, setConnectionStep, setHttpPort, setSocksPort, setStatus, t]);

  const connectionOpRef = useRef(0);
  const [testingSubId, setTestingSubId] = useState<string | null>(null);
  const [refreshingSubId, setRefreshingSubId] = useState<string | null>(null);
  const [pingingServerIds, setPingingServerIds] = useState<Set<string>>(() => new Set());
  const serverSelectionIndex = useMemo(() => buildServerSelectionIndex(servers), [servers]);
  const autoPingStartedRef = useRef<Set<string>>(new Set());
  const autoSubRefreshStartedRef = useRef(false);
  const trafficLimitNoticeKeyRef = useRef<string | null>(null);
  const eofBurstRef = useRef({ count: 0, windowStartedAt: 0, lastNoticeAt: 0 });
  const tRef = useRef(t);

  useEffect(() => {
    tRef.current = t;
  }, [t]);

  const [confirmModal, setConfirmModal] = useState<{
    show: boolean;
    title: string;
    message: string;
    onConfirm: () => void;
    confirmLabel?: string;
    danger?: boolean;
  }>({ show: false, title: '', message: '', onConfirm: () => {} });

  // ═══════════════════════════════════════════════════
  //  Effects
  // ═══════════════════════════════════════════════════

  // Auto-ping unpinged servers after persisted state/subscriptions are loaded.
  useEffect(() => {
    const unpinged = servers.filter(
      s => (s.ping === undefined || (s.ping > 0 && s.ping <= 5)) && !autoPingStartedRef.current.has(s.id)
    );
    if (unpinged.length === 0) return;
    for (const server of unpinged) {
      autoPingStartedRef.current.add(server.id);
    }
    let cancelled = false;
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await pingServersWithLimit(unpinged, invoke, {
          isCancelled: () => cancelled,
          onBatch: (updates) => updateServerPings(updates),
        });
      } catch { /* not in tauri env */ }
    })();
    return () => { cancelled = true; };
  }, [servers, updateServerPings]);

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

  // Keep persisted selections aligned with refreshed subscription server ids.
  useEffect(() => {
    if (!activeServer) return;
    const matchedServer = findMatchingServerInIndex(activeServer, serverSelectionIndex);
    if (matchedServer && matchedServer.id !== activeServer.id) {
      setActiveServer(matchedServer);
    }
  }, [activeServer, serverSelectionIndex, setActiveServer]);

  // Sync connection state from backend on mount
  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const running: boolean = await invoke('vpn_status');
        if (running && status !== 'connected') {
          const health = await invoke('get_connection_health', {
            proxyMode,
            systemProxyMode,
            socksPort,
            httpPort,
          }) as ConnectionHealthReport;
          if (isHealthAcceptable(proxyMode, health)) {
            const healthPorts = extractPortsFromHealth(health);
            if (healthPorts.socksPort) setSocksPort(healthPorts.socksPort);
            if (healthPorts.httpPort) setHttpPort(healthPorts.httpPort);
            setStatus('connected');
            addLog('info', 'VPN is still active (reconnected after UI reload)');
          } else {
            setStatus('disconnected');
            addLog('warning', `Backend reported VPN active, but health is not acceptable: ${summarizeHealthFailures(health)}`);
          }
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
        const health = await invoke('get_connection_health', {
          proxyMode,
          systemProxyMode,
          socksPort,
          httpPort,
        }) as ConnectionHealthReport;
        const healthPorts = extractPortsFromHealth(health);
        if (healthPorts.socksPort) setSocksPort(healthPorts.socksPort);
        if (healthPorts.httpPort) setHttpPort(healthPorts.httpPort);
        const healthy = isHealthAcceptable(proxyMode, health);
        if (healthy) { healthFailRef.current = 0; }
        else {
          healthFailRef.current++;
          if (healthFailRef.current >= 3) {
            const healthMessage = proxyMode === 'tun'
              ? 'Protected-mode health quorum is unstable; keeping the tunnel up and monitoring...'
              : 'Proxy health check is unstable; keeping the connection up and monitoring...';
            addLog('warning', healthMessage);
            const toastStoreModule = await import('../stores/toast-store');
            toastStoreModule.useToastStore.getState().addToast('Connection health is unstable; monitoring...', 'warning');
            const activeHealthServer = useAppStore.getState().activeServer;
            reportConnectionError({
              eventType: 'health_drop', serverName: activeHealthServer?.name,
              serverAddress: activeHealthServer?.address, serverPort: activeHealthServer?.port,
              protocol: activeHealthServer?.protocol,
              errorMessage: proxyMode === 'tun'
                ? `Protected health quorum unstable: ${summarizeHealthFailures(health)}`
                : `Proxy health unstable: ${summarizeHealthFailures(health)}`,
            });
            healthFailRef.current = 0;
            return;
          }
        }
      } catch { /* not in tauri env */ }
    }, 30000);
    return () => clearInterval(healthCheck);
  }, [status, socksPort, httpPort, proxyMode, systemProxyMode]); // eslint-disable-line react-hooks/exhaustive-deps

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
          if (isBenignProxyTeardownLine(line)) continue;
          if (isProxyResponseEofLine(line)) {
            const state = useAppStore.getState();
            const activeSub = getSubscriptionById(state.subscriptions, state.activeServer?.subscriptionId);
            const trafficStatus = activeSub ? getSubscriptionTrafficStatus(activeSub) : null;
            const now = Date.now();
            const burst = eofBurstRef.current;

            if (now - burst.windowStartedAt > TRAFFIC_LIMIT_EOF_WINDOW_MS) {
              burst.windowStartedAt = now;
              burst.count = 1;
            } else {
              burst.count += 1;
            }

            const shouldShowNotice = !!trafficStatus?.isLimited || burst.count >= TRAFFIC_LIMIT_EOF_THRESHOLD;
            if (shouldShowNotice && now - burst.lastNoticeAt > TRAFFIC_LIMIT_NOTICE_COOLDOWN_MS) {
              const message = trafficStatus?.isLimited
                ? `${trafficStatus.reason === 'expired' ? tRef.current('subscriptionExpiredLog') : tRef.current('subscriptionTrafficLimitedLog')}${activeSub ? `: ${activeSub.name}` : ''}`
                : tRef.current('subscriptionMaybeLimitedLog');
              burst.lastNoticeAt = now;
              addLog('warning', message);
              const { useToastStore } = await import('../stores/toast-store');
              useToastStore.getState().addToast(message, 'warning');
            }
            continue;
          }

          const level = getProxyLogLevel(line);
          if (level) {
            addLog(level, line);
          }
        }
      } catch { /* */ }
    }, 1000);
    return () => clearInterval(poll);
  }, [status, addLog]);

  // Show a clear status as soon as the active subscription reports no remaining traffic.
  useEffect(() => {
    if (status !== 'connected') {
      trafficLimitNoticeKeyRef.current = null;
      return;
    }

    const activeSub = getSubscriptionById(subscriptions, activeServer?.subscriptionId);
    if (!activeSub) return;

    const trafficStatus = getSubscriptionTrafficStatus(activeSub);
    if (!trafficStatus.isLimited) {
      trafficLimitNoticeKeyRef.current = null;
      return;
    }

    const key = `${activeSub.id}:${activeSub.updatedAt}:${trafficStatus.reason}`;
    if (trafficLimitNoticeKeyRef.current === key) return;
    trafficLimitNoticeKeyRef.current = key;

    const message = `${trafficStatus.reason === 'expired' ? t('subscriptionExpiredLog') : t('subscriptionTrafficLimitedLog')}: ${activeSub.name}`;
    eofBurstRef.current.lastNoticeAt = Date.now();
    addLog('warning', message);
    import('../stores/toast-store').then(({ useToastStore }) => {
      useToastStore.getState().addToast(message, 'warning');
    }).catch(() => {});
  }, [status, activeServer?.subscriptionId, subscriptions, addLog, t]);

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

  // Auto-detect clipboard links removed to prevent macOS permission spam
  // Subscription auto-update
  useEffect(() => {
    if (subAutoUpdateMinutes <= 0 || subscriptions.length === 0) return;

    const refreshAllSubscriptions = async (logSuccess: boolean) => {
      for (const sub of subscriptions) {
        try {
          const updated = await refreshSubscription(sub);
          updateSubscription(sub.id, updated);
          if (logSuccess) {
            addLog('info', `Auto-updated subscription: ${sub.name} (${updated.servers.length} servers)`);
          }
        } catch (err: any) {
          const message = err?.message || String(err);
          if (logSuccess) {
            addLog('error', `Auto-update failed for ${sub.name}: ${message}`);
          }
          reportConnectionError({
            eventType: subscriptionErrorEventType(message),
            errorMessage: message,
            details: { action: 'auto_update_subscription', subscription: sub.name },
          });
        }
      }
    };

    if (!autoSubRefreshStartedRef.current) {
      autoSubRefreshStartedRef.current = true;
      refreshAllSubscriptions(false);
    }

    const interval = setInterval(async () => {
      refreshAllSubscriptions(true);
    }, subAutoUpdateMinutes * 60 * 1000);
    return () => clearInterval(interval);
  }, [subAutoUpdateMinutes, subscriptions, updateSubscription, addLog]);

  // ═══════════════════════════════════════════════════
  //  Handlers
  // ═══════════════════════════════════════════════════

  const handleConnect = useCallback(async () => {
    if (status === 'disconnected') {
      const opId = ++connectionOpRef.current;
      const activeRoutingRules = await getActiveRoutingRules();
      if (proxyMode !== 'tun' && activeRoutingRules.length > 0) {
        const message = formatMessage(t('splitTunnelingProxyWarning'), { count: activeRoutingRules.length });
        addLog('warning', message);
        reportConnectionError({
          eventType: 'split_rule_ignored',
          errorMessage: message,
          details: {
            active_rules: activeRoutingRules.length,
            proxy_mode: proxyMode,
          },
        });
        try {
          const { useToastStore } = await import('../stores/toast-store');
          useToastStore.getState().addToast(t('splitTunnelingNeedsTun'), 'warning');
        } catch { /* ignore */ }
      }

      const srv = resolveConnectServer(activeServer, servers, autoSelectFastest);
      if (srv && findMatchingServer(activeServer, [srv]) === null) {
        setActiveServer(srv);
        if (!activeServer && autoSelectFastest && srv.ping !== undefined && srv.ping > 0) {
          addLog('info', `Auto-selected fastest: ${srv.name} (${srv.ping}ms)`);
        }
      }
      if (!srv) { addLog('error', 'No server selected. Please add a subscription or select a server.'); return; }
      setStatus('connecting');
      setConnectionStep(t('connectionStarting'));

      if (proxyMode === 'tun') {
        addLog('info', 'Режим «Весь компьютер»: DoodleRay управляет сетевым адаптером через свой сервис.');
        void refreshTunnelServiceHealth();
      }

      setConnectedAt(null);
      addLog('info', `Starting connection to ${srv.name}...`);

      try {
        const { invoke } = await import('@tauri-apps/api/core');
        setConnectionStep(t('connectionCheckingServer'));
        const request = await buildConnectRequestFromState(srv);
        setConnectionStep(t('connectionSecuringTraffic'));
        const result: any = await withTimeout(
          invoke('vpn_connect', { request }),
          CONNECT_TIMEOUT_MS,
          'Connection timed out while starting VPN engines'
        );

        if (opId !== connectionOpRef.current) return;

        if (result.success) {
          await markConnectedIfHealthy(
            result,
            invoke,
            proxyMode,
            request.system_proxy_mode,
            request.socks_port,
            request.http_port,
          );
          return;
        } else {
          // Port-busy retry
          if (result.message.includes('bind') || result.message.includes('10808') || result.message.includes('port')) {
            try {
              const portInfo: any = await invoke('check_port', { port: socksPort });
              if (portInfo.busy && portInfo.doodleray_owned) {
                addLog('warning', 'Fixing connection route automatically...');
                await invoke('force_free_port', { port: socksPort });
                await new Promise(r => setTimeout(r, 1000));
                const retryReq = await buildConnectRequestFromState(srv!);
                const retry: any = await invoke('vpn_connect', { request: retryReq });
                if (opId !== connectionOpRef.current) return;
                if (retry.success) {
                  await markConnectedIfHealthy(
                    retry,
                    invoke,
                    proxyMode,
                    retryReq.system_proxy_mode,
                    retryReq.socks_port,
                    retryReq.http_port,
                  );
                  return;
                }
              } else if (portInfo.busy) {
                addLog('warning', `${portInfo.message}. Change local proxy ports in Settings, for example SOCKS5 20808 and HTTP 20809.`);
              }
            } catch {}
          }
          addLog('error', result.message);
          if (result.message.toLowerCase().includes('full computer components')) {
            const serviceHealthy = await refreshTunnelServiceHealth();
            if (!serviceHealthy) {
              addLog('warning', 'Tunnel service is not ready. Please reinstall or repair DoodleRay from the installer/settings diagnostics.');
            }
          }
          reportConnectionError({ eventType: 'connect_fail', serverName: srv!.name, serverAddress: srv!.address, serverPort: srv!.port, protocol: srv!.protocol, errorMessage: result.message });
          setStatus('disconnected');
          setConnectionStep(null);
        }
      } catch (err: any) {
        if (opId !== connectionOpRef.current) return;
        if (isTauriRuntime()) {
          const message = err.message || String(err);
          addLog('error', `Connection failed: ${message}`);
          try {
            const { invoke: cleanupInvoke } = await import('@tauri-apps/api/core');
            await cleanupInvoke('vpn_disconnect');
          } catch { /* best effort cleanup */ }
          reportConnectionError({
            eventType: 'connect_fail',
            serverName: srv!.name,
            serverAddress: srv!.address,
            serverPort: srv!.port,
            protocol: srv!.protocol,
            errorMessage: message,
          });
          setStatus('disconnected');
          setConnectionStep(null);
          setCurrentSpeed(0, 0);
          resetTraffic();
        } else {
          addLog('warning', `Dev mode - simulating connection: ${err.message || err}`);
          setConnectionStep(t('connectionSecuringTraffic'));
          setTimeout(() => { addLog('success', `[SIM] Connected via ${srv!.protocol.toUpperCase()}+${srv!.transport}`); setConnectionStep(t('connectionReady')); setStatus('connected'); setConnectedAt(Date.now()); }, 1500);
        }
      }
    } else if (status === 'connecting') {
      ++connectionOpRef.current;
      addLog('warning', 'Cancelling connection start...');
      setStatus('disconnecting');
      setConnectionStep(t('connectionDisconnecting'));
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result: any = await invoke('vpn_disconnect');
        addLog(result.success ? 'info' : 'error', result.message);
      } catch { addLog('info', '[SIM] Disconnected'); }
      setStatus('disconnected'); setConnectionStep(null); setConnectedAt(null); setCurrentSpeed(0, 0); resetTraffic();
    } else if (status === 'connected') {
      addLog('warning', 'Disconnecting...');
      ++connectionOpRef.current;
      setStatus('disconnecting');
      setConnectionStep(t('connectionDisconnecting'));
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const result: any = await invoke('vpn_disconnect');
        addLog(result.success ? 'info' : 'error', result.message);
      } catch { addLog('info', '[SIM] Disconnected'); }
      setStatus('disconnected'); setConnectionStep(null); setConnectedAt(null); setCurrentSpeed(0, 0); resetTraffic();
    }
  }, [status, setStatus, setCurrentSpeed, resetTraffic, activeServer, servers, setActiveServer, addLog, proxyMode, socksPort, httpPort, autoSelectFastest, setConnectedAt, t, setProxyMode, refreshTunnelServiceHealth, setSocksPort, setHttpPort, markConnectedIfHealthy]);

  const handleModeSwitch = useCallback(async (mode: 'system-proxy' | 'tun', nextSystemProxyMode = systemProxyMode) => {
    const normalizedSystemProxyMode = nextSystemProxyMode === 'clear'
      ? 'unchanged'
      : nextSystemProxyMode;
    const modeChanged = proxyMode !== mode;
    const systemProxyChanged = systemProxyMode !== normalizedSystemProxyMode;
    if (!modeChanged && !systemProxyChanged) return;
    if (mode === 'tun') {
      addLog('info', 'Режим «Весь компьютер» будет использовать сервис DoodleRay для сетевого адаптера.');
      await refreshTunnelServiceHealth();
    }
    setProxyMode(mode);
    if (systemProxyChanged) setSystemProxyMode(normalizedSystemProxyMode);
    addLog('info', `Режим подключения: ${mode === 'tun' ? t('fullDeviceMode') : t('systemProxy')}`);
    if (status === 'connected') {
      addLog('warning', 'Reconnecting to apply new routing mode...');
      setStatus('connecting');
      setConnectionStep(t('connectionSecuringTraffic'));
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('vpn_disconnect');
        await new Promise(r => setTimeout(r, 2000));
        const srv = activeServer;
        if (srv) {
          const request = await buildConnectRequestFromState(srv, mode, normalizedSystemProxyMode);
          const result: any = await invoke('vpn_connect', { request });
          if (result.success) {
            await markConnectedIfHealthy(
              result,
              invoke,
              mode,
              request.system_proxy_mode,
              request.socks_port,
              request.http_port,
            );
          }
          else { addLog('error', result.message); setStatus('disconnected'); setConnectionStep(null); }
        } else { setStatus('disconnected'); setConnectionStep(null); }
      } catch (err: any) { addLog('error', `Reconnect failed: ${err.message || err}`); setStatus('disconnected'); setConnectionStep(null); }
    }
  }, [proxyMode, systemProxyMode, setProxyMode, setSystemProxyMode, status, setStatus, addLog, activeServer, socksPort, httpPort, setConnectedAt, t, refreshTunnelServiceHealth, setSocksPort, setHttpPort, markConnectedIfHealthy]);

  const handleServerSelect = useCallback(async (server: typeof activeServer) => {
    if (!server) return;
    const isSameServer = findMatchingServer(activeServer, [server]) !== null;
    setActiveServer(server); setSearchQuery('');
    if (status === 'connected' && !isSameServer) {
      addLog('warning', `Switching to ${server.name}...`);
      setStatus('connecting');
      setConnectionStep(t('connectionCheckingServer'));
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        // Don't call vpn_disconnect — vpn_connect handles cleanup internally
        // and preserves the TUN bridge to avoid game disconnections
        const request = await buildConnectRequestFromState(server);
        const result: any = await invoke('vpn_connect', { request });
        if (result.success) {
          await markConnectedIfHealthy(
            result,
            invoke,
            proxyMode,
            request.system_proxy_mode,
            request.socks_port,
            request.http_port,
          );
        }
        else { addLog('error', result.message); setStatus('disconnected'); setConnectionStep(null); }
      } catch (err: any) { addLog('error', `Server switch failed: ${err.message || err}`); setStatus('disconnected'); setConnectionStep(null); }
    }
  }, [status, setStatus, activeServer, setActiveServer, addLog, proxyMode, socksPort, httpPort, setConnectedAt, t, setSocksPort, setHttpPort, markConnectedIfHealthy]);

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
    } catch (err: any) {
      const message = err.message || String(err);
      addLog('error', `Error: ${message}`);
      reportConnectionError({
        eventType: subscriptionErrorEventType(message),
        errorMessage: message,
        details: { action: 'quick_add' },
      });
    }
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
      await pingServersWithLimit(toUpdate.filter((s) => s.address), invoke, {
        onActiveIdsChange: setPingingServerIds,
        onBatch: (updates) => useAppStore.getState().updateServerPings(updates),
      });
      addLog('success', 'Ping test complete');
    } catch (err: any) { addLog('error', `Ping test failed: ${err?.message || err}`); }
    finally { setPingingServerIds(new Set()); setTestingSubId(null); }
  };

  const handleUpdateSubscription = async (sub: any) => {
    setRefreshingSubId(sub.id);
    try {
      addLog('info', `Updating subscription: ${sub.name}...`);
      const updated = await refreshSubscription(sub);
      updateSubscription(sub.id, updated);
      addLog('success', `Updated ${sub.name}: ${updated.servers.length} servers`);
    } catch (err: any) {
      const message = err.message || String(err);
      addLog('error', `Failed to update ${sub.name}: ${message}`);
      reportConnectionError({
        eventType: subscriptionErrorEventType(message),
        errorMessage: message,
        details: { action: 'manual_update_subscription', subscription: sub.name },
      });
    }
    finally { setRefreshingSubId(null); }
  };

  const handleTestCustomServers = async () => {
    setTestingSubId('__custom__');
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const custom = servers.filter(s => !s.subscriptionId);
      addLog('warning', `Testing ${custom.length} custom servers...`);
      await pingServersWithLimit(custom.filter((s) => s.address), invoke, {
        onActiveIdsChange: setPingingServerIds,
        onBatch: (updates) => useAppStore.getState().updateServerPings(updates),
      });
      addLog('success', 'Custom servers ping test complete');
    } catch (err: any) { addLog('error', `Custom ping test failed: ${err?.message || err}`); }
    finally { setPingingServerIds(new Set()); setTestingSubId(null); }
  };

  const handleRemoveServer = useCallback((serverId: string, serverName: string) => {
    setConfirmModal({
      show: true,
      title: t('deleteServer'),
      message: `Delete custom server "${serverName}"?`,
      confirmLabel: t('deleteServer').split(' ')[0],
      danger: true,
      onConfirm: () => {
        if (activeServer?.id === serverId) { handleConnect(); setActiveServer(null); }
        removeServer(serverId);
        setConfirmModal(prev => ({ ...prev, show: false }));
      }
    });
  }, [activeServer, handleConnect, setActiveServer, removeServer, t]);

  const handleRemoveAllCustom = useCallback(() => {
    setConfirmModal({
      show: true,
      title: t('deleteAllProfiles'),
      message: 'Delete all manual profiles?',
      confirmLabel: t('deleteServer').split(' ')[0],
      danger: true,
      onConfirm: () => {
        removeAllManualServers();
        addLog('info', 'Removed all custom servers');
        setConfirmModal(prev => ({ ...prev, show: false }));
      }
    });
  }, [removeAllManualServers, addLog, t]);

  const handleRemoveSubscription = useCallback((subId: string) => {
    const sub = subscriptions.find(s => s.id === subId);
    if (!sub) return;
    setConfirmModal({
      show: true,
      title: t('deleteSub'),
      message: `Delete subscription "${sub.name}" and all its servers?`,
      confirmLabel: t('deleteServer').split(' ')[0],
      danger: true,
      onConfirm: () => {
        removeSubscription(subId);
        setConfirmModal(prev => ({ ...prev, show: false }));
      }
    });
  }, [subscriptions, removeSubscription, t]);

  const canConnect = !!activeServer || servers.length > 0;
  const hasDashboardContent = servers.length > 0 || status !== 'disconnected';
  const trimmedQuickInput = quickInput.trim();
  const quickInputKind = trimmedQuickInput.startsWith('http://') || trimmedQuickInput.startsWith('https://')
    ? 'subscription'
    : /^(vless|vmess|trojan|ss|hy2|tuic|wg):\/\//.test(trimmedQuickInput)
      ? 'link'
      : 'unknown';
  const quickInputHint = !trimmedQuickInput
    ? t('detectedUnknown')
    : quickInputKind === 'subscription'
      ? t('detectedSubscription')
      : quickInputKind === 'link'
        ? t('detectedProxyLink')
        : t('detectedUnknown');
  const connectionStepLabel = status === 'connecting' || status === 'disconnecting' ? connectionStep : null;

  // ═══════════════════════════════════════════════════
  //  Render
  // ═══════════════════════════════════════════════════
  return (
    <div className="relative flex-1 flex flex-col overflow-hidden animate-fade-in">
      <RetroBackground />

      <div className="relative z-10 flex-1 flex flex-col items-center gap-3 px-4 overflow-y-auto py-4">
        {/* + Add button */}
        <button
          onClick={() => setShowAddModal(!showAddModal)}
          disabled={quickImporting}
          className="absolute top-4 right-4 z-30 w-10 h-10 flex items-center justify-center bg-white border-[3px] border-black rounded-xl shadow-[3px_3px_0_#000] cursor-pointer hover:translate-x-[-1px] hover:translate-y-[-1px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none transition-all disabled:opacity-50"
          title={t('addSubOrServer')}
        >
          {quickImporting ? <Loader2 className="w-5 h-5 text-black animate-spin stroke-[3px]" /> : <Plus className="w-5 h-5 text-black stroke-[3px]" />}
        </button>

        {showAddModal && (
          <>
            <div
              className="fixed inset-0 z-30 bg-black/20 backdrop-blur-[1px]"
              onClick={() => setShowAddModal(false)}
            />
            <div className="fixed left-1/2 top-16 z-40 w-[min(360px,calc(100vw-32px))] -translate-x-1/2 bg-white border-[3px] border-black rounded-2xl p-4 shadow-[6px_6px_0_#000] animate-slide-up space-y-3">
              <div>
                <p className="text-[11px] font-black text-black uppercase tracking-widest">{t('pasteToAddTitle')}</p>
                <p className="mt-1 text-[10px] font-bold leading-relaxed text-black/55 uppercase tracking-widest">{t('pasteToAddDesc')}</p>
              </div>
              <div className="rounded-xl border-[3px] border-black bg-bg-primary p-2 shadow-inner">
                <div className="flex gap-2">
                  <input type="text" value={quickInput} onChange={(e) => setQuickInput(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter' && quickInputKind !== 'unknown') { handleQuickAdd(); setShowAddModal(false); } }}
                    autoFocus placeholder={t('pasteHint')}
                    className="flex-1 min-w-0 bg-white border-[2px] border-black rounded-lg px-3 py-2.5 text-xs text-black placeholder:text-black/30 focus:outline-none font-bold tracking-tight" />
                  <button onClick={handleQuickPaste}
                    className="w-10 h-10 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer hover:bg-black hover:text-white transition-colors shrink-0">
                    <ClipboardPaste className="w-4 h-4 stroke-[2.5px]" />
                  </button>
                </div>
                <p className={`mt-2 inline-flex rounded-lg border-[2px] border-black px-2 py-1 text-[9px] font-black uppercase tracking-widest ${
                  quickInputKind === 'subscription'
                    ? 'bg-emerald-300 text-black'
                    : quickInputKind === 'link'
                      ? 'bg-amber-300 text-black'
                      : 'bg-white/70 text-black/45'
                }`}>
                  {quickInputHint}
                </p>
              </div>
              <button onClick={() => { handleQuickAdd(); setShowAddModal(false); }}
                disabled={quickImporting || !trimmedQuickInput || quickInputKind === 'unknown'}
                className="w-full py-2.5 bg-black text-white border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-1 active:shadow-none transition-all disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2">
                {quickImporting ? <><Loader2 className="w-3.5 h-3.5 animate-spin" /> {t('adding')}</> : <><Plus className="w-3.5 h-3.5 stroke-[3px]" /> {t('add')}</>}
              </button>
            </div>
          </>
        )}

        {/* ═══ MAIN CONTENT ═══ */}
        {!hasDashboardContent ? (
          <OnboardingCard
            quickInput={quickInput} setQuickInput={setQuickInput}
            onQuickAdd={handleQuickAdd} onQuickPaste={handleQuickPaste}
            importing={quickImporting} t={t}
          />
        ) : (
          <div className="contents">
            <ConnectionControls
              status={status} canConnect={canConnect}
              connectionStepLabel={connectionStepLabel}
              onConnect={handleConnect}
              t={t}
            />

            <ServerList
              status={status}
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
              pingingServerIds={pingingServerIds}
              subAutoUpdateMinutes={subAutoUpdateMinutes}
              t={t}
            />
          </div>
        )}
      </div>

      {hasDashboardContent && (
        <div className="relative z-20 flex shrink-0 justify-center px-4 pb-2 pt-2">
          <DashboardControlsDrawer
            status={status} proxyMode={proxyMode} systemProxyMode={systemProxyMode}
            connectTime={connectTime}
            currentDownload={currentDownload} currentUpload={currentUpload}
            totalDown={totalDown} totalUp={totalUp}
            socksPort={socksPort} httpPort={httpPort}
            speedHistory={speedHistory} showStats={showStats}
            onModeSwitch={handleModeSwitch}
            t={t}
          />
        </div>
      )}

      <LogsStrip
        logs={logs} showLogs={showLogs}
        onToggleLogs={() => setShowLogs(!showLogs)}
        onClearLogs={clearLogs} logsEndRef={logsEndRef} t={t}
      />

      {/* ── CUSTOM CONFIRM MODAL ── */}
      {confirmModal.show && (
        <>
          <div className="fixed inset-0 z-[60] bg-black/40 backdrop-blur-sm"
            onClick={() => setConfirmModal(prev => ({ ...prev, show: false }))} />
          <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[70] w-72 bg-white border-[3px] border-black rounded-2xl p-5 shadow-[6px_6px_0_#000] animate-slide-up flex flex-col gap-4">
            <div>
              <h3 className="text-xs font-black uppercase tracking-widest leading-tight">{confirmModal.title}</h3>
              <p className="text-xs text-black/60 font-bold mt-2 leading-relaxed">{confirmModal.message}</p>
            </div>
            <div className="flex gap-2 mt-2">
              <button 
                onClick={() => setConfirmModal(prev => ({ ...prev, show: false }))}
                className="flex-1 py-2 bg-white text-black border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:bg-black/5 hover:-translate-y-0.5 active:translate-y-0 transition-all">
                {t('cancel')}
              </button>
              <button 
                onClick={confirmModal.onConfirm}
                className={`flex-1 py-2 border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[2px_2px_0_#000] hover:shadow-[3px_3px_0_#000] hover:-translate-y-0.5 active:translate-y-1 active:shadow-none transition-all ${
                  confirmModal.danger ? 'bg-danger text-white' : 'bg-black text-white'
                }`}>
                {confirmModal.confirmLabel || 'OK'}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
