import type { ServerConfig } from '../stores/app-store';

function normalizePart(value: unknown): string {
  return String(value ?? '').trim().toLowerCase();
}

export function getServerIdentityKey(server: ServerConfig): string {
  return [
    normalizePart(server.protocol),
    normalizePart(server.address),
    normalizePart(server.port),
    normalizePart(server.name),
    normalizePart(server.transport),
    normalizePart(server.security),
  ].join('|');
}

export function findMatchingServer(
  target: ServerConfig | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  if (!target) return null;

  const byId = servers.find((server) => server.id === target.id);
  if (byId) return byId;

  const targetKey = getServerIdentityKey(target);
  return servers.find((server) => getServerIdentityKey(server) === targetKey) || null;
}

export function selectPreferredServer(
  servers: ServerConfig[],
  autoSelectFastest: boolean,
): ServerConfig | null {
  if (servers.length === 0) return null;
  if (!autoSelectFastest) return servers[0];

  const withPing = servers.filter((server) => server.ping !== undefined && server.ping > 0);
  if (withPing.length === 0) return servers[0];

  return withPing.reduce((best, server) => (server.ping! < best.ping! ? server : best));
}

export function resolveConnectServer(
  activeServer: ServerConfig | null,
  servers: ServerConfig[],
  autoSelectFastest: boolean,
): ServerConfig | null {
  return findMatchingServer(activeServer, servers) || selectPreferredServer(servers, autoSelectFastest);
}
