import type { ProxyMode, SystemProxyMode } from '../stores/app-store';

export type ConnectionHealthSeverity = 'ok' | 'info' | 'warning' | 'error';

export type ConnectionHealthCheck = {
  code: string;
  severity: ConnectionHealthSeverity | string;
  title: string;
  detail: string;
};

export type ConnectionHealthReport = {
  verdict: 'protected' | 'partial' | 'failed' | string;
  mode: string;
  generated_at_ms: number;
  checks: ConnectionHealthCheck[];
};

export type ConnectionHealthPorts = { socksPort?: number; httpPort?: number };

export function extractPortsFromHealth(health: ConnectionHealthReport | null | undefined): ConnectionHealthPorts {
  const ports: ConnectionHealthPorts = {};
  for (const check of health?.checks ?? []) {
    const match = check.detail.match(/127\.0\.0\.1:(\d+)/);
    if (!match) continue;
    const port = Number(match[1]);
    if (!Number.isInteger(port) || port <= 0 || port > 65535) continue;
    if (check.code === 'socks_listener') ports.socksPort = port;
    if (check.code === 'http_listener') ports.httpPort = port;
  }
  return ports;
}

export function isHealthAcceptable(mode: ProxyMode, health: ConnectionHealthReport | null | undefined): boolean {
  if (!health) return false;
  if (mode === 'tun') return health.verdict === 'protected';
  return health.verdict !== 'failed';
}

export function summarizeHealthFailures(health: ConnectionHealthReport | null | undefined): string {
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
