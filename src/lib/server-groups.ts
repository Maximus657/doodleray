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

function normalizeWhitespace(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

export function getServerGroupLabel(server: ServerConfig): string {
  const label = normalizeWhitespace(
    server.name
      .replace(FLAG_EMOJI_REGEX, '')
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

function getDisplayRank(label: string): number {
  const normalized = label.toLowerCase();
  if (/авто|самый быстрый|auto|entry-pool|fastest/.test(normalized)) return 0;
  if (/обход|блокиров|white|whitelist|bypass/.test(normalized)) return 1;
  if (/резерв|reserve/.test(normalized)) return 2;
  if (/нидерланд|netherlands/.test(normalized)) return 10;
  if (/герман|germany/.test(normalized)) return 11;
  if (/польш|poland/.test(normalized)) return 12;
  if (/росси|russia/.test(normalized)) return 13;
  if (/казах|kazakhstan/.test(normalized)) return 14;
  if (/сша|united states/.test(normalized)) return 15;
  return 50;
}

interface MutableServerDisplayGroup extends ServerDisplayGroup {
  bestPositivePing?: number;
  displayRank: number;
  originalIndex: number;
  allFailed: boolean;
}

export function buildServerDisplayGroups(servers: ServerConfig[]): ServerDisplayGroup[] {
  const groups = new Map<string, MutableServerDisplayGroup>();

  for (const server of servers) {
    const label = getServerGroupLabel(server);
    const key = [
      label.toLowerCase(),
      server.subscriptionId || 'manual',
    ].join('|');
    const group = groups.get(key);
    const ping = server.ping;

    if (group) {
      group.servers.push(server);
      if (!group.countryCode && server.countryCode) group.countryCode = server.countryCode;
      if (ping !== -1) group.allFailed = false;
      if (ping !== undefined && ping > 0 && (group.bestPositivePing === undefined || ping < group.bestPositivePing)) {
        group.bestPositivePing = ping;
        group.ping = ping;
        group.selectedServer = server;
      }
      continue;
    }

    const hasPositivePing = ping !== undefined && ping > 0;
    groups.set(key, {
      id: key,
      label,
      servers: [server],
      selectedServer: server,
      countryCode: server.countryCode,
      ping: hasPositivePing ? ping : undefined,
      bestPositivePing: hasPositivePing ? ping : undefined,
      displayRank: getDisplayRank(label),
      originalIndex: groups.size,
      allFailed: ping === -1,
    });
  }

  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      ping: group.bestPositivePing ?? (group.allFailed ? -1 : undefined),
    }))
    .sort((a, b) => a.displayRank - b.displayRank || a.originalIndex - b.originalIndex)
    .map(({
      bestPositivePing: _bestPositivePing,
      displayRank: _displayRank,
      originalIndex: _originalIndex,
      allFailed: _allFailed,
      ...group
    }) => group);
}
