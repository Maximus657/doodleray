import type { ProxyMode, SystemProxyMode } from '../stores/app-store';

export type ConnectionHealthSeverity = 'ok' | 'info' | 'warning' | 'error';

export type ConnectionHealthCheck = {
  code: string;
  severity: ConnectionHealthSeverity | string;
  title: string;
  detail: string;
};

export type ConnectionHealthReport = {
  verdict: 'protected' | 'protected_degraded' | 'limited' | 'repairing' | 'partial' | 'failed' | 'cleanup_pending' | string;
  mode: string;
  generated_at_ms: number;
  service_effective_state?: string;
  service_health_verdict?: string;
  engine_kind?: string;
  runtime_socks_port?: number;
  runtime_http_port?: number;
  runtime_api_port?: number;
  service_generation?: number;
  active_op_id?: string;
  service_fatal_checks?: string[];
  service_degraded_checks?: string[];
  service_warning_checks?: string[];
  route_explanations?: string[];
  endpoint_bypass_checks?: string[];
  checks: ConnectionHealthCheck[];
};

export type ConnectionHealthPorts = { socksPort?: number; httpPort?: number };

export function extractPortsFromHealth(health: ConnectionHealthReport | null | undefined): ConnectionHealthPorts {
  const ports: ConnectionHealthPorts = {};
  const runtimeSocksPort = health?.runtime_socks_port;
  const runtimeHttpPort = health?.runtime_http_port;
  if (isValidPort(runtimeSocksPort)) ports.socksPort = runtimeSocksPort;
  if (isValidPort(runtimeHttpPort)) ports.httpPort = runtimeHttpPort;
  return ports;
}

function isValidPort(port: unknown): port is number {
  return Number.isInteger(port) && Number(port) > 0 && Number(port) <= 65535;
}

export function isHealthAcceptable(mode: ProxyMode, health: ConnectionHealthReport | null | undefined): boolean {
  if (!health) return false;
  if (mode === 'tun') return health.verdict === 'protected' || health.verdict === 'protected_degraded';
  return health.verdict !== 'failed' && health.verdict !== 'cleanup_pending';
}

const NON_ACTIONABLE_PROTECTED_DEGRADED_RE =
  /(ipv6 full-protection leak proof is not collected|degraded_disabled|quic\/http3 is not verified|quic.*not verified|unverified-no-tooling)/i;

function failedHealthLines(health: ConnectionHealthReport): string[] {
  const failedChecks = (health.checks ?? [])
    .filter(check => check.severity === 'error' || check.severity === 'warning')
    .map(check => `${check.code} ${check.title} ${check.detail}`);
  return [
    ...(health.service_fatal_checks ?? []),
    ...(health.service_degraded_checks ?? []),
    ...(health.service_warning_checks ?? []),
    ...failedChecks,
  ].filter(Boolean);
}

export function isNonActionableProtectedDegraded(health: ConnectionHealthReport | null | undefined): boolean {
  if (!health || health.verdict !== 'protected_degraded') return false;
  if ((health.service_fatal_checks ?? []).length > 0) return false;

  const failed = failedHealthLines(health);
  if (failed.length === 0) return false;
  return failed.every(line => NON_ACTIONABLE_PROTECTED_DEGRADED_RE.test(line));
}

export function getUserVisibleHealthVerdict(health: ConnectionHealthReport | null | undefined): string | null {
  return health?.verdict ?? null;
}

export function isHealthFatal(mode: ProxyMode, health: ConnectionHealthReport | null | undefined): boolean {
  if (!health || (health.verdict !== 'failed' && health.verdict !== 'cleanup_pending')) return false;
  if (mode !== 'tun') return false;
  if ((health.service_fatal_checks ?? []).length > 0) return true;

  return (health.checks ?? []).some(check => {
    if (check.severity !== 'error') return false;
    if (check.code === 'tunnel_service_fatal_checks') return true;
    if (check.code !== 'tunnel_service') return false;
    return /state=(?:Failed|Disconnected)/.test(check.detail);
  });
}

export function needsProtectedRuntimeRepair(health: ConnectionHealthReport | null | undefined): boolean {
  if (!health) return false;
  const effective = String(health.service_effective_state ?? '').toLowerCase();
  if (health.verdict === 'repairing') return true;
  if (effective === 'suspect' || effective === 'repairing') return true;
  return (health.service_degraded_checks ?? []).some(check =>
    /network\/power event|route reassertion|runtime repair/i.test(check)
  );
}

export function summarizeHealthFailures(health: ConnectionHealthReport | null | undefined): string {
  if (isNonActionableProtectedDegraded(health)) {
    return 'non-actionable IPv6/QUIC verification warnings';
  }
  const failed = (health?.checks ?? []).filter(check => check.severity === 'error' || check.severity === 'warning');
  if (failed.length === 0) return `health verdict=${health?.verdict ?? 'missing'}`;
  return failed.slice(0, 3).map(check => `${check.title}: ${check.detail}`).join('; ');
}

type InvokeFn = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

export async function waitForConnectionHealth(
  invoke: InvokeFn,
  mode: ProxyMode,
  systemProxyMode: SystemProxyMode,
  fallbackSocksPort: number,
  fallbackHttpPort: number,
  initialHealth?: ConnectionHealthReport | null,
  attempts = mode === 'tun' ? 8 : 3,
  delayMs = mode === 'tun' ? 1500 : 700,
): Promise<{ health: ConnectionHealthReport | null; socksPort: number; httpPort: number }> {
  let health = initialHealth ?? null;
  let socksPort = fallbackSocksPort;
  let httpPort = fallbackHttpPort;

  for (let attempt = 0; attempt < attempts; attempt++) {
    const healthPorts = extractPortsFromHealth(health);
    socksPort = healthPorts.socksPort ?? socksPort;
    httpPort = healthPorts.httpPort ?? httpPort;
    if (isHealthAcceptable(mode, health)) break;

    if (attempt > 0 || !health) {
      health = await invoke('get_connection_health', {
        proxyMode: mode,
        systemProxyMode,
        socksPort,
        httpPort,
      }) as ConnectionHealthReport;
      const latestPorts = extractPortsFromHealth(health);
      socksPort = latestPorts.socksPort ?? socksPort;
      httpPort = latestPorts.httpPort ?? httpPort;
      if (isHealthAcceptable(mode, health)) break;
    }

    if (attempt < attempts - 1) await delay(delayMs);
  }

  return { health, socksPort, httpPort };
}
