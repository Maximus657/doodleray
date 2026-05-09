import type { ServerConfig } from '../stores/app-store';

export interface ServerDisplayGroup {
  id: string;
  label: string;
  servers: ServerConfig[];
  selectedServer: ServerConfig;
  countryCode?: string;
  ping?: number;
}

const FLAG_EMOJI_REGEX = /[\u{1F1E6}-\u{1F1FF}]{2}/gu;
const NUMBERED_SUFFIX_REGEX = /\s*(?:[-–—]|#|№)\s*\d+\s*$/u;

function normalizeWhitespace(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

export function getServerGroupLabel(server: ServerConfig): string {
  const label = normalizeWhitespace(
    server.name
      .replace(FLAG_EMOJI_REGEX, '')
      .replace(NUMBERED_SUFFIX_REGEX, '')
  );

  return label || server.country || server.name;
}

export function serverMatchesGroupQuery(server: ServerConfig, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return true;

  return [
    server.name,
    getServerGroupLabel(server),
    server.country,
    server.countryCode,
  ].some((value) => value?.toLowerCase().includes(normalizedQuery));
}

function selectBestServer(servers: ServerConfig[]): ServerConfig {
  const withPing = servers.filter((server) => server.ping !== undefined && server.ping > 0);
  if (withPing.length === 0) return servers[0];
  return withPing.reduce((best, server) => (server.ping! < best.ping! ? server : best));
}

function getBestGroupPing(servers: ServerConfig[]): number | undefined {
  const positivePings = servers
    .map((server) => server.ping)
    .filter((ping): ping is number => ping !== undefined && ping > 0);

  if (positivePings.length > 0) return Math.min(...positivePings);
  if (servers.length > 0 && servers.every((server) => server.ping === -1)) return -1;
  return undefined;
}

export function buildServerDisplayGroups(servers: ServerConfig[]): ServerDisplayGroup[] {
  const groups = new Map<string, ServerDisplayGroup>();

  for (const server of servers) {
    const label = getServerGroupLabel(server);
    const key = label.toLowerCase();
    const group = groups.get(key);

    if (group) {
      group.servers.push(server);
      if (!group.countryCode && server.countryCode) group.countryCode = server.countryCode;
      continue;
    }

    groups.set(key, {
      id: `${key}-${server.subscriptionId || 'manual'}`,
      label,
      servers: [server],
      selectedServer: server,
      countryCode: server.countryCode,
    });
  }

  return Array.from(groups.values()).map((group) => ({
    ...group,
    selectedServer: selectBestServer(group.servers),
    ping: getBestGroupPing(group.servers),
  }));
}
