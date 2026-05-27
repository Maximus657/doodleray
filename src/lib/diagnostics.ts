import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../stores/app-store';
import { getActiveRoutingRules } from './connect-helpers';

export type DiagnosticSeverity = 'ok' | 'info' | 'warning' | 'error';

export interface DiagnosticCheck {
  severity: DiagnosticSeverity;
  code: string;
  title: string;
  detail: string;
}

export interface NetworkDiagnosticsReport {
  summary: 'ok' | 'warnings_found' | 'errors_found';
  subscriptionHost?: string | null;
  resolvedIps: string[];
  conflicts: Array<{ name: string; reason: string }>;
  checks: DiagnosticCheck[];
  durationMs?: number;
}

export interface StoragePathReport {
  label: string;
  path: string;
  kind: string;
  exists: boolean;
  clearable: boolean;
  bytes: number;
  size: string;
  truncated?: boolean;
}

export interface StorageReport {
  totalBytes: number;
  totalSize: string;
  paths: StoragePathReport[];
}

export interface CacheClearReport {
  removed: Array<{
    label: string;
    path: string;
    kind: string;
    bytes: number;
    size: string;
  }>;
  failed: Array<{ label: string; path: string; error: string }>;
}

export async function runNetworkDiagnostics(subscriptionUrl?: string | null) {
  const state = useAppStore.getState();
  const routingRules = await getActiveRoutingRules();
  return invoke<NetworkDiagnosticsReport>('run_network_diagnostics', {
    subscriptionUrl: subscriptionUrl || null,
    socksPort: state.socksPort,
    httpPort: state.httpPort,
    activeServerAddress: state.activeServer?.address || null,
    activeServerPort: state.activeServer?.port || null,
    activeServerProtocol: state.activeServer?.protocol || null,
    proxyMode: state.proxyMode,
    appStatus: state.status,
    activeRoutingRuleCount: routingRules.length,
    systemProxyMode: state.systemProxyMode,
    dnsMode: state.dnsMode,
    networkStack: state.networkStack,
  });
}

export async function getStorageReport() {
  return invoke<StorageReport>('get_storage_report');
}

export async function clearAppCache() {
  return invoke<CacheClearReport>('clear_app_cache');
}

export function diagnosticsReportToText(report: NetworkDiagnosticsReport): string {
  const lines = [
    `DoodleRay Network Diagnostics`,
    `Summary: ${report.summary}`,
  ];
  if (typeof report.durationMs === 'number') lines.push(`Duration: ${report.durationMs} ms`);
  if (report.subscriptionHost) lines.push(`Subscription host: ${report.subscriptionHost}`);
  if (report.resolvedIps.length > 0) lines.push(`Resolved IPs: ${report.resolvedIps.join(', ')}`);
  if (report.conflicts.length > 0) {
    lines.push(`Conflicts: ${report.conflicts.map((item) => item.name).join(', ')}`);
  }
  lines.push('');
  for (const check of report.checks) {
    lines.push(`[${check.severity.toUpperCase()}] ${check.title}`);
    lines.push(check.detail);
  }
  return lines.join('\n');
}
