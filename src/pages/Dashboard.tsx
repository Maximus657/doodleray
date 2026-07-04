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
  isHealthFatal,
  needsProtectedRuntimeRepair,
  summarizeHealthFailures,
  waitForConnectionHealth,
  type ConnectionHealthReport,
} from '../lib/connection-health';
import { buildServerSelectionIndex, findMatchingServer, findMatchingServerInIndex, resolveConnectServer } from '../lib/server-selection';
import { getSubscriptionById, getSubscriptionTrafficStatus } from '../lib/subscription-status';
import { pingServersWithLimit } from '../lib/ping-runner';
import { describeSubscriptionSource } from '../lib/redaction';

// v6 DoodleVPN design UI
import type { ProductMode } from '../stores/app-store';
import ConnectOrb from '../components/v6/ConnectOrb';
import { displayServerName } from '../components/v6/ServerRow';
import ModeSelector from '../components/v6/ModeCard';
import LocationList from '../components/v6/LocationList';
import TrafficStats from '../components/v6/TrafficStats';
import SplitRoutingToggle from '../components/v6/SplitRoutingToggle';
import SplitRoutingModal from '../components/v6/SplitRoutingModal';
import DiagnosticsDrawer from '../components/v6/DiagnosticsDrawer';
import DiagnosticPanel from '../components/v6/DiagnosticPanel';
import QuickAddPanel from '../components/v6/QuickAddPanel';
import { deriveOrbState, ORB_LABEL_KEY } from '../components/v6/status';

const TRAFFIC_LIMIT_EOF_WINDOW_MS = 12_000;
const TRAFFIC_LIMIT_EOF_THRESHOLD = 4;
const TRAFFIC_LIMIT_NOTICE_COOLDOWN_MS = 60_000;
const CONNECT_TIMEOUT_MS = 45_000;
const TUN_CONNECT_TIMEOUT_MS = 120_000;
const TUN_LIMITED_FALLBACK_RE =
  /could not create the Windows tunnel adapter|IPv4 readiness failed|adapter is missing|adapter did not become ready|route is not preferred|route did not become ready|routes are missing|sing-box exited|sing-box process is not running|Tunnel Service failed to start TUN|Tunnel Service stopped before TUN|Tunnel Service did not become ready|timed out while starting VPN engines/i;

function isTunLimitedFallbackCandidate(message?: string | null) {
  return !!message && TUN_LIMITED_FALLBACK_RE.test(message);
}

function hasWinInetCompatibilityWarning(health: ConnectionHealthReport | null | undefined): boolean {
  return (health?.checks ?? []).some(check =>
    check.code === 'wininet_proxy' && (check.severity === 'warning' || check.severity === 'error')
  );
}

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

export default function Dashboard() {
  const {
    status, setStatus, activeServer, servers, setActiveServer,
    proxyMode, setProxyMode, systemProxyMode, setSystemProxyMode, productMode, currentDownload, currentUpload,
    addTraffic, resetTraffic, addSpeedPoint, setCurrentSpeed,
    logs, addLog, clearLogs, socksPort, httpPort, subscriptions,
    updateSubscription, autoSelectFastest,
    subAutoUpdateMinutes, connectedAt, setConnectedAt,
    addSubscription, addServer,
    updateServerPings, setSocksPort, setHttpPort, showStats,
  } = useAppStore();
  const { t } = useTranslation();

  const [searchQuery, setSearchQuery] = useState('');
  const [healthVerdict, setHealthVerdict] = useState<string | null>(null);
  const [quickInput, setQuickInput] = useState('');
  const [quickImporting, setQuickImporting] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  const [showSplitModal, setShowSplitModal] = useState(false);
  const [showDiagModal, setShowDiagModal] = useState(false);
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
    let effectiveSocksPort = fallbackSocksPort;
    let effectiveHttpPort = fallbackHttpPort;

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
      const failureSummary = summarizeHealthFailures(health);
      addLog('error', `Connection started but health quorum failed: ${failureSummary}`);
      try {
        const bundlePath = await invoke('export_support_bundle', {
          proxyMode: mode,
          systemProxyMode: nextSystemProxyMode,
          socksPort: effectiveSocksPort,
          httpPort: effectiveHttpPort,
          failureMarker: `connect_health_failed: ${failureSummary}`,
        }) as string;
        addLog('info', `${t('supportBundleExported')}: ${bundlePath}`);
      } catch (bundleErr: any) {
        addLog('warning', `${t('supportBundleExportFailed')}: ${bundleErr?.message || bundleErr}`);
      }
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
    setHealthVerdict(health?.verdict ?? null);
    setStatus('connected');
    setConnectedAt(Date.now());
    return true;
  }, [addLog, setConnectedAt, setConnectionStep, setHttpPort, setSocksPort, setStatus, t]);

  const connectionOpRef = useRef(0);
  const [pingingServerIds, setPingingServerIds] = useState<Set<string>>(() => new Set());
  const serverSelectionIndex = useMemo(() => buildServerSelectionIndex(servers), [servers]);
  const autoPingStartedRef = useRef<Set<string>>(new Set());
  const autoSubRefreshStartedRef = useRef(false);
  const trafficLimitNoticeKeyRef = useRef<string | null>(null);
  const eofBurstRef = useRef({ count: 0, windowStartedAt: 0, lastNoticeAt: 0 });
  const tRef = useRef(t);

  const attemptLimitedBrowsersFallback = useCallback(async (
    srv: NonNullable<typeof activeServer>,
    invoke: any,
    opId: number,
    reason: string,
  ) => {
    if (proxyMode !== 'tun' || !isTunLimitedFallbackCandidate(reason)) return false;
    addLog('warning', t('limitedFallbackAttempt'));
    try {
      // Force the failed protected generation to clean up before starting the
      // lightweight browser compatibility path. This avoids reusing a failed
      // service/TUN state as the fallback substrate.
      await invoke('vpn_disconnect').catch(() => undefined);
      await new Promise(resolve => setTimeout(resolve, 1500));
      setProxyMode('system-proxy');
      setSystemProxyMode('set');
      const fbReq = await buildConnectRequestFromState(srv, 'system-proxy', 'set');
      const fb: any = await invoke('vpn_connect', { request: fbReq });
      if (opId !== connectionOpRef.current) return true;
      if (fb.success) {
        await markConnectedIfHealthy(
          fb,
          invoke,
          'system-proxy',
          fbReq.system_proxy_mode,
          fbReq.socks_port,
          fbReq.http_port,
        );
        addLog('warning', t('limitedFallbackActive'));
        const { useToastStore } = await import('../stores/toast-store');
        useToastStore.getState().addToast(t('limitedFallbackActive'), 'warning');
        return true;
      }
      addLog('error', fb.message);
    } catch (fbErr: any) {
      addLog('error', `Browsers fallback failed: ${fbErr?.message || fbErr}`);
    }
    return false;
  }, [addLog, markConnectedIfHealthy, proxyMode, setProxyMode, setSystemProxyMode, t]);

  useEffect(() => {
    tRef.current = t;
  }, [t]);

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
          onActiveIdsChange: setPingingServerIds,
          onBatch: (updates) => updateServerPings(updates),
        });
      } catch { /* not in tauri env */ }
      finally { setPingingServerIds(new Set()); }
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
          let health = await invoke('get_connection_health', {
            proxyMode,
            systemProxyMode,
            socksPort,
            httpPort,
          }) as ConnectionHealthReport;
          if (isHealthAcceptable(proxyMode, health)) {
            const healthPorts = extractPortsFromHealth(health);
            const effectiveSocksPort = healthPorts.socksPort ?? socksPort;
            const effectiveHttpPort = healthPorts.httpPort ?? httpPort;
            if (healthPorts.socksPort) setSocksPort(healthPorts.socksPort);
            if (healthPorts.httpPort) setHttpPort(healthPorts.httpPort);

            if (
              proxyMode === 'tun' &&
              systemProxyMode === 'set'
            ) {
              try {
                const repairMessage = await invoke('repair_active_tunnel_compatibility_proxy', {
                  systemProxyMode,
                }) as string;
                addLog('debug', `Browser compatibility repaired after UI reload: ${repairMessage}`);
                health = await invoke('get_connection_health', {
                  proxyMode,
                  systemProxyMode,
                  socksPort: effectiveSocksPort,
                  httpPort: effectiveHttpPort,
                }) as ConnectionHealthReport;
              } catch (repairError) {
                addLog('warning', `Browser compatibility repair after UI reload failed: ${repairError instanceof Error ? repairError.message : String(repairError)}`);
              }
            }

            setStatus('connected');
            addLog(
              'debug',
              hasWinInetCompatibilityWarning(health)
                ? `VPN is still active after UI reload, but browser compatibility is degraded: ${summarizeHealthFailures(health)}`
                : 'VPN is still active (reconnected after UI reload)',
            );
          } else {
            setStatus('disconnected');
            addLog('debug', `Backend reported VPN active, but health is not acceptable: ${summarizeHealthFailures(health)}`);
          }
        }
      } catch { /* not in tauri env */ }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Connection health monitor
  const healthFailRef = useRef(0);
  const healthInFlightRef = useRef(false);
  const runtimeRepairRef = useRef({ inFlight: false, lastAt: 0 });
  const compatRepairRef = useRef({ inFlight: false, lastAt: 0 });
  const fatalWatchdogRef = useRef(false);
  useEffect(() => {
    if (status !== 'connected') {
      healthFailRef.current = 0;
      healthInFlightRef.current = false;
      runtimeRepairRef.current = { inFlight: false, lastAt: 0 };
      compatRepairRef.current = { inFlight: false, lastAt: 0 };
      fatalWatchdogRef.current = false;
      return;
    }
    const healthCheck = setInterval(async () => {
      if (healthInFlightRef.current) return;
      healthInFlightRef.current = true;
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        let health = await withTimeout(
          invoke('get_connection_health', {
            proxyMode,
            systemProxyMode,
            socksPort,
            httpPort,
          }) as Promise<ConnectionHealthReport>,
          proxyMode === 'tun' ? 12000 : 6000,
          'Connection health check timed out',
        );
        const healthPorts = extractPortsFromHealth(health);
        if (healthPorts.socksPort) setSocksPort(healthPorts.socksPort);
        if (healthPorts.httpPort) setHttpPort(healthPorts.httpPort);

        if (
          proxyMode === 'tun' &&
          needsProtectedRuntimeRepair(health) &&
          !runtimeRepairRef.current.inFlight &&
          Date.now() - runtimeRepairRef.current.lastAt > 20_000
        ) {
          runtimeRepairRef.current.inFlight = true;
          runtimeRepairRef.current.lastAt = Date.now();
          try {
            const repairMessage = await invoke('repair_active_tunnel_runtime', {
              reason: 'ui_health_monitor',
            }) as string;
            addLog('info', repairMessage);
            if (systemProxyMode === 'set') {
              try {
                const compatMessage = await invoke('repair_active_tunnel_compatibility_proxy', {
                  systemProxyMode,
                }) as string;
                addLog('info', compatMessage);
              } catch (compatErr: any) {
                addLog('warning', `Browser compatibility repair is still pending: ${compatErr?.message || compatErr}`);
              }
            }
            health = await invoke('get_connection_health', {
              proxyMode,
              systemProxyMode,
              socksPort: healthPorts.socksPort ?? socksPort,
              httpPort: healthPorts.httpPort ?? httpPort,
            }) as ConnectionHealthReport;
            const repairedPorts = extractPortsFromHealth(health);
            if (repairedPorts.socksPort) setSocksPort(repairedPorts.socksPort);
            if (repairedPorts.httpPort) setHttpPort(repairedPorts.httpPort);
          } catch (repairErr: any) {
            addLog('warning', `Runtime repair did not complete: ${repairErr?.message || repairErr}`);
          } finally {
            runtimeRepairRef.current.inFlight = false;
          }
        }

        const compatibilityNeedsRepair = proxyMode === 'tun' &&
          systemProxyMode === 'set' &&
          (hasWinInetCompatibilityWarning(health) ||
            (health.service_degraded_checks ?? []).some(check => /Windows proxy compatibility/i.test(check)));
        if (
          compatibilityNeedsRepair &&
          !compatRepairRef.current.inFlight &&
          Date.now() - compatRepairRef.current.lastAt > 20_000
        ) {
          compatRepairRef.current.inFlight = true;
          compatRepairRef.current.lastAt = Date.now();
          try {
            const compatMessage = await invoke('repair_active_tunnel_compatibility_proxy', {
              systemProxyMode,
            }) as string;
            addLog('info', compatMessage);
            health = await invoke('get_connection_health', {
              proxyMode,
              systemProxyMode,
              socksPort: healthPorts.socksPort ?? socksPort,
              httpPort: healthPorts.httpPort ?? httpPort,
            }) as ConnectionHealthReport;
            const repairedPorts = extractPortsFromHealth(health);
            if (repairedPorts.socksPort) setSocksPort(repairedPorts.socksPort);
            if (repairedPorts.httpPort) setHttpPort(repairedPorts.httpPort);
          } catch (compatErr: any) {
            addLog('warning', `Browser compatibility repair is still pending: ${compatErr?.message || compatErr}`);
          } finally {
            compatRepairRef.current.inFlight = false;
          }
        }

        const healthy = isHealthAcceptable(proxyMode, health);
        setHealthVerdict(health?.verdict ?? null);
        if (healthy) { healthFailRef.current = 0; }
        else if (isHealthFatal(proxyMode, health)) {
          const failureSummary = summarizeHealthFailures(health);
          healthFailRef.current = 0;
          addLog('error', `Whole computer mode stopped: ${failureSummary}`);
          try {
            const bundlePath = await invoke('export_support_bundle', {
              proxyMode,
              systemProxyMode,
              socksPort,
              httpPort,
              failureMarker: `health_fatal: ${failureSummary}`,
            }) as string;
            addLog('info', `${t('supportBundleExported')}: ${bundlePath}`);
          } catch (bundleErr: any) {
            addLog('warning', `${t('supportBundleExportFailed')}: ${bundleErr?.message || bundleErr}`);
          }
          try { await invoke('vpn_disconnect'); } catch { /* best effort cleanup */ }
          setStatus('disconnected');
          setConnectionStep(null);
          setConnectedAt(null);
          const toastStoreModule = await import('../stores/toast-store');
          toastStoreModule.useToastStore.getState().addToast('Whole computer mode stopped; reconnect to repair it.', 'error');
          const activeHealthServer = useAppStore.getState().activeServer;
          reportConnectionError({
            eventType: 'health_fatal', serverName: activeHealthServer?.name,
            serverAddress: activeHealthServer?.address, serverPort: activeHealthServer?.port,
            protocol: activeHealthServer?.protocol,
            errorMessage: `Protected health fatal: ${failureSummary}`,
          });
          return;
        }
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
      } catch {
        healthFailRef.current++;
        if (healthFailRef.current >= 3) {
          addLog('warning', 'Connection health check is delayed; keeping the connection up and monitoring...');
          healthFailRef.current = 0;
        }
      } finally {
        healthInFlightRef.current = false;
      }
    }, 30000);
    return () => clearInterval(healthCheck);
  }, [status, socksPort, httpPort, proxyMode, systemProxyMode]); // eslint-disable-line react-hooks/exhaustive-deps

  // Fast protected-mode fatal watchdog. The normal health monitor is broader
  // and intentionally gentle; this one only consumes service-owned runtime
  // truth so a crashed TUN core cannot leave the UI green for a full monitor
  // interval.
  useEffect(() => {
    if (status !== 'connected' || proxyMode !== 'tun') return;
    let disposed = false;
    const checkFatal = async () => {
      if (disposed || fatalWatchdogRef.current) return;
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const health = await withTimeout(
          invoke('get_connection_health', {
            proxyMode,
            systemProxyMode,
            socksPort,
            httpPort,
          }) as Promise<ConnectionHealthReport>,
          6000,
          'Protected watchdog timed out',
        );
        setHealthVerdict(health?.verdict ?? null);
        if (!isHealthFatal('tun', health)) return;

        fatalWatchdogRef.current = true;
        const failureSummary = summarizeHealthFailures(health);
        addLog('error', `Whole computer mode stopped: ${failureSummary}`);
        try { await invoke('vpn_disconnect'); } catch { /* best effort cleanup */ }
        setStatus('disconnected');
        setConnectionStep(null);
        setConnectedAt(null);
        const toastStoreModule = await import('../stores/toast-store');
        toastStoreModule.useToastStore.getState().addToast('Whole computer mode stopped; reconnect to repair it.', 'error');
      } catch {
        // The slower monitor handles repeated timeouts. Avoid noisy duplicate logs here.
      }
    };
    const first = window.setTimeout(checkFatal, 1500);
    const timer = window.setInterval(checkFatal, 4000);
    return () => {
      disposed = true;
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [status, proxyMode, systemProxyMode, socksPort, httpPort, addLog, setStatus, setConnectionStep, setConnectedAt]);

  // Reset the orb health verdict whenever the tunnel is fully down.
  useEffect(() => {
    if (status === 'disconnected') setHealthVerdict(null);
  }, [status]);

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
        addLog('debug', 'Режим «Весь компьютер»: DoodleRay управляет сетевым адаптером через свой сервис.');
        void refreshTunnelServiceHealth();
      }

      setConnectedAt(null);
      addLog('info', `Starting connection to ${srv.name}...`);

      try {
        const { invoke } = await import('@tauri-apps/api/core');
        setConnectionStep(t('connectionCheckingServer'));
        const request = await buildConnectRequestFromState(srv);
        setConnectionStep(t('connectionSecuringTraffic'));
        const connectTimeoutMs = proxyMode === 'tun' ? TUN_CONNECT_TIMEOUT_MS : CONNECT_TIMEOUT_MS;
        const result: any = await withTimeout(
          invoke('vpn_connect', { request }),
          connectTimeoutMs,
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
          // Honest automatic fallback: a TUN adapter/route bring-up failure
          // (already past the service-side bounded repair) degrades to
          // Browsers compatibility with explicit limited-protection messaging.
          // Manual mode is never entered automatically; WinINet is only
          // touched by the Browsers connect path itself.
          if (await attemptLimitedBrowsersFallback(srv!, invoke, opId, result.message)) {
            return;
          }
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
            let fallbackReason = message;
            try {
              const health = await cleanupInvoke('get_connection_health', {
                proxyMode: 'tun',
                systemProxyMode,
                socksPort,
                httpPort,
              }) as ConnectionHealthReport;
              fallbackReason = `${fallbackReason}; ${summarizeHealthFailures(health)}`;
            } catch { /* best effort */ }
            if (srv && await attemptLimitedBrowsersFallback(srv, cleanupInvoke, opId, fallbackReason)) {
              return;
            }
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
  }, [status, setStatus, setCurrentSpeed, resetTraffic, activeServer, servers, setActiveServer, addLog, proxyMode, socksPort, httpPort, autoSelectFastest, setConnectedAt, t, setProxyMode, setSystemProxyMode, refreshTunnelServiceHealth, setSocksPort, setHttpPort, markConnectedIfHealthy, attemptLimitedBrowsersFallback]);

  const handleModeSwitch = useCallback(async (mode: 'system-proxy' | 'tun', nextSystemProxyMode = systemProxyMode) => {
    const normalizedSystemProxyMode = nextSystemProxyMode === 'clear'
      ? 'unchanged'
      : nextSystemProxyMode;
    const modeChanged = proxyMode !== mode;
    const systemProxyChanged = systemProxyMode !== normalizedSystemProxyMode;
    if (!modeChanged && !systemProxyChanged) return;
    if (mode === 'tun') {
      addLog('debug', 'Режим «Весь компьютер» будет использовать сервис DoodleRay для сетевого адаптера.');
      await refreshTunnelServiceHealth();
    }
    setProxyMode(mode);
    if (systemProxyChanged) setSystemProxyMode(normalizedSystemProxyMode);
    addLog('debug', `Режим подключения: ${mode === 'tun' ? t('fullDeviceMode') : t('systemProxy')}`);
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
          const connectTimeoutMs = mode === 'tun' ? TUN_CONNECT_TIMEOUT_MS : CONNECT_TIMEOUT_MS;
          const result: any = await withTimeout(
            invoke('vpn_connect', { request }),
            connectTimeoutMs,
            'Connection timed out while starting VPN engines',
          );
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
          else {
            addLog('error', result.message);
            if (mode === 'tun' && await attemptLimitedBrowsersFallback(srv, invoke, connectionOpRef.current, result.message)) {
              return;
            }
            setStatus('disconnected');
            setConnectionStep(null);
          }
        } else { setStatus('disconnected'); setConnectionStep(null); }
      } catch (err: any) {
        const message = err.message || String(err);
        addLog('error', `Reconnect failed: ${message}`);
        if (mode === 'tun' && activeServer) {
          try {
            const { invoke: cleanupInvoke } = await import('@tauri-apps/api/core');
            if (await attemptLimitedBrowsersFallback(activeServer, cleanupInvoke, connectionOpRef.current, message)) {
              return;
            }
          } catch { /* best effort fallback */ }
        }
        setStatus('disconnected');
        setConnectionStep(null);
      }
    }
  }, [proxyMode, systemProxyMode, setProxyMode, setSystemProxyMode, status, setStatus, addLog, activeServer, socksPort, httpPort, setConnectedAt, t, refreshTunnelServiceHealth, setSocksPort, setHttpPort, markConnectedIfHealthy, attemptLimitedBrowsersFallback]);

  const handleExportSupportBundle = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const path = await invoke('export_support_bundle', {
        proxyMode,
        systemProxyMode,
        socksPort,
        httpPort,
      }) as string;
      addLog('success', `${t('supportBundleExported')}: ${path}`);
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast(`${t('supportBundleExported')}: ${path}`, 'success');
    } catch (err: any) {
      addLog('error', `${t('supportBundleExportFailed')}: ${err?.message || err}`);
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast(`${t('supportBundleExportFailed')}: ${err?.message || err}`, 'error');
    }
  }, [addLog, httpPort, proxyMode, socksPort, systemProxyMode, t]);

  // QA-only control surface consumer (backend gates it behind
  // DOODLERAY_QA_CONTROL=1; production launches never enable it). Actions are
  // executed through the exact same handlers the UI buttons use.
  const qaControlRef = useRef({ status, handleConnect, handleModeSwitch });
  useEffect(() => {
    qaControlRef.current = { status, handleConnect, handleModeSwitch };
  });
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const enabled = await invoke<boolean>('qa_control_enabled').catch(() => false);
        if (!enabled || disposed) return;
        const publishFrontendSnapshot = async () => {
          const state = useAppStore.getState();
          await invoke('qa_control_update_frontend_snapshot', {
            snapshot: {
              status: state.status,
              product_mode: state.productMode,
              proxy_mode: state.proxyMode,
              system_proxy_mode: state.systemProxyMode,
              subscriptions_count: state.subscriptions.length,
              servers_count: state.servers.length,
              active_server_present: !!state.activeServer,
              active_server_name: state.activeServer?.name ?? null,
              active_server_protocol: state.activeServer?.protocol ?? null,
              socks_port: state.socksPort,
              http_port: state.httpPort,
              recent_logs: state.logs.slice(-25).map(log => ({
                level: log.level,
                message: log.message,
              })),
            },
          }).catch(() => undefined);
        };
        await publishFrontendSnapshot();
        const snapshotTimer = window.setInterval(publishFrontendSnapshot, 1000);
        const { listen } = await import('@tauri-apps/api/event');
        const un = await listen<{ action?: string; query?: string }>('doodleray-qa-control', async (event) => {
          const current = qaControlRef.current;
          const action = event.payload?.action;
          const query = new URLSearchParams(event.payload?.query || '');
          const loggableQuery = action === 'import-subscription' ? '' : (event.payload?.query || '');
          addLog('info', `[QA-control] ${action}${loggableQuery ? `?${loggableQuery}` : ''}`);
          try {
            if (action === 'connect') {
              if (current.status !== 'disconnected') {
                await current.handleConnect();
                await new Promise(resolve => setTimeout(resolve, 5000));
              }
              if (qaControlRef.current.status === 'disconnected') {
                await qaControlRef.current.handleConnect();
              }
            } else if (action === 'disconnect') {
              if (current.status !== 'disconnected') {
                await current.handleConnect();
              }
            } else if (action === 'switch-mode') {
              const mode = query.get('mode');
              if (mode === 'tun') await current.handleModeSwitch('tun', 'set');
              else if (mode === 'browsers') await current.handleModeSwitch('system-proxy', 'set');
              else if (mode === 'manual') await current.handleModeSwitch('system-proxy', 'unchanged');
            } else if (action === 'refresh-subscription') {
              const { refreshSubscription } = await import('../lib/subscription');
              const storeModule = await import('../stores/app-store');
              const state = storeModule.useAppStore.getState();
              for (const sub of state.subscriptions) {
                const updated = await refreshSubscription(sub);
                state.updateSubscription(sub.id, updated);
              }
              addLog('success', '[QA-control] subscriptions refreshed');
            } else if (action === 'import-subscription') {
              const url = query.get('url');
              if (url) {
                const { fetchSubscription } = await import('../lib/subscription');
                const storeModule = await import('../stores/app-store');
                const state = storeModule.useAppStore.getState();
                const existing = state.subscriptions.find((sub) => sub.url === url);
                if (existing) {
                  const { refreshSubscription } = await import('../lib/subscription');
                  state.updateSubscription(existing.id, await refreshSubscription(existing));
                  addLog('success', '[QA-control] existing subscription refreshed');
                } else {
                  state.addSubscription(await fetchSubscription(url));
                  addLog('success', '[QA-control] subscription imported');
                }
              }
            }
          } catch (err: any) {
            addLog('error', `[QA-control] ${action} failed: ${err?.message || err}`);
          }
        });
        if (disposed) {
          window.clearInterval(snapshotTimer);
          un();
        }
        else unlisten = () => { window.clearInterval(snapshotTimer); un(); };
      } catch { /* QA surface unavailable */ }
    })();
    return () => { disposed = true; unlisten?.(); };
  }, [addLog]);

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
        addLog('info', `Fetching subscription: ${describeSubscriptionSource(trimmed)}`);
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

  const handlePingAll = useCallback(async () => {
    const toPing = servers.filter((s) => s.address);
    if (toPing.length === 0) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await pingServersWithLimit(toPing, invoke, {
        onActiveIdsChange: setPingingServerIds,
        onBatch: (updates) => useAppStore.getState().updateServerPings(updates),
      });
    } catch { /* not in tauri env */ }
    finally { setPingingServerIds(new Set()); }
  }, [servers]);

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
  //  Render (DoodleVPN design)
  // ═══════════════════════════════════════════════════
  const busy = status === 'connecting' || status === 'disconnecting';
  const connected = status === 'connected';
  const orbState = deriveOrbState(status, productMode, healthVerdict);
  const fmtTimer = (s: number) => {
    const h = Math.floor(s / 3600);
    const m = String(Math.floor((s % 3600) / 60)).padStart(2, '0');
    const sec = String(s % 60).padStart(2, '0');
    return h > 0 ? `${h}:${m}:${sec}` : `${m}:${sec}`;
  };
  const orbPrimary = connected ? t('disconnect') : busy ? '···' : t('connect');
  const orbSub = busy
    ? (connectionStepLabel || t('connecting'))
    : connected
      ? fmtTimer(connectTime)
      : null;
  const orbStatusLabel = orbState === 'protected'
    ? t('v6Encrypted' as never)
    : t(ORB_LABEL_KEY[orbState] as never);
  const activeSub = getSubscriptionById(subscriptions, activeServer?.subscriptionId) ?? subscriptions[0] ?? null;

  const handleModeSelect = (mode: ProductMode) => {
    if (mode === 'protected') handleModeSwitch('tun', 'set');
    else if (mode === 'compatibility') handleModeSwitch('system-proxy', 'set');
    else handleModeSwitch('system-proxy', 'unchanged');
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {showAddModal && (
        <QuickAddPanel
          value={quickInput}
          onChange={setQuickInput}
          onAdd={() => { handleQuickAdd(); setShowAddModal(false); }}
          onPaste={handleQuickPaste}
          onClose={() => setShowAddModal(false)}
          importing={quickImporting}
          kind={quickInputKind}
          hint={quickInputHint}
          t={t}
        />
      )}

      {!hasDashboardContent ? (
        <div className="flex min-h-0 flex-1 items-center justify-center p-6">
          <div className="v6-glass w-full max-w-md rounded-[26px] p-7 text-center">
            <div
              className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-[15px]"
              style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)', boxShadow: '0 6px 24px rgba(234,109,6,0.45)' }}
            >
              <span className="h-[18px] w-[18px] rounded-full border-[3px] border-white" />
            </div>
            <h2 className="text-[18px] font-semibold text-white">{t('welcome')}</h2>
            <p className="mx-auto mt-1.5 max-w-xs text-[12.5px] leading-relaxed text-white/50">{t('welcomeHint')}</p>
            <div className="mt-4 flex gap-2">
              <input
                type="text"
                value={quickInput}
                onChange={(e) => setQuickInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter' && quickInputKind !== 'unknown') handleQuickAdd(); }}
                placeholder={t('pasteHint')}
                className="v6-glass-inset min-w-0 flex-1 rounded-[14px] px-3.5 py-2.5 text-[13px] text-white outline-none placeholder:text-white/40 v6-focus"
              />
              <button
                type="button"
                onClick={handleQuickPaste}
                aria-label="Paste"
                className="v6-hover-bright grid h-[42px] w-[42px] shrink-0 place-items-center rounded-[13px] border border-white/[0.12] bg-white/[0.07] text-white/70 v6-focus"
              >
                <ClipboardPaste className="h-4 w-4" strokeWidth={2.2} />
              </button>
            </div>
            <button
              type="button"
              onClick={handleQuickAdd}
              disabled={quickImporting || !trimmedQuickInput || quickInputKind === 'unknown'}
              className="mt-2.5 flex w-full items-center justify-center gap-2 rounded-[14px] py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40 v6-focus"
              style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)', boxShadow: '0 6px 18px rgba(234,109,6,0.35)' }}
            >
              {quickImporting ? <><Loader2 className="h-4 w-4 v6-orb-spin" /> {t('adding')}</> : <><Plus className="h-4 w-4" strokeWidth={2.6} /> {t('add')}</>}
            </button>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-[22px]">
          <LocationList
            servers={servers}
            activeServer={activeServer}
            activeSub={activeSub}
            pingingServerIds={pingingServerIds}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onSelect={handleServerSelect}
            onAdd={() => setShowAddModal(true)}
            onPingAll={handlePingAll}
            t={t}
          />

          {/* RIGHT: modes, connect core, bottom row */}
          <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
            <ModeSelector current={productMode} onSelect={handleModeSelect} disabled={busy} t={t} />

            <ConnectOrb
              state={orbState}
              primaryLabel={orbPrimary}
              subLabel={orbSub}
              statusLabel={orbStatusLabel}
              serverName={activeServer ? displayServerName(activeServer) : null}
              serverCountryCode={activeServer?.countryCode ?? null}
              disabled={status === 'disconnected' && !canConnect}
              onClick={handleConnect}
              onDiagnose={() => setShowDiagModal(true)}
              diagnoseLabel={t('v6DiagIssueCta' as never)}
            />

            <div className="flex shrink-0 gap-3.5">
              <SplitRoutingToggle protectedMode={productMode === 'protected'} onOpen={() => setShowSplitModal(true)} t={t} />
              {showStats && (
                <TrafficStats
                  connected={connected}
                  currentDownload={currentDownload}
                  currentUpload={currentUpload}
                  t={t}
                />
              )}
            </div>

            <DiagnosticsDrawer
              logs={logs}
              onClear={clearLogs}
              onOpenDiagnostics={() => setShowDiagModal(true)}
              t={t}
            />
          </div>
        </div>
      )}

      {showDiagModal && (
        <DiagnosticPanel
          onClose={() => setShowDiagModal(false)}
          onExportSupportBundle={handleExportSupportBundle}
          t={t}
        />
      )}

      {showSplitModal && (
        <SplitRoutingModal
          protectedMode={productMode === 'protected'}
          onClose={() => setShowSplitModal(false)}
          t={t}
        />
      )}
    </div>
  );
}
