import type { ServerConfig } from '../stores/app-store';

function normalizePart(value: unknown): string {
  return String(value ?? '').trim().toLowerCase();
}

function normalizeTransportPath(server: ServerConfig): string {
  const path = normalizePart(server.path);
  if (path) return path;

  const transport = normalizePart(server.transport);
  if (transport === 'ws' || transport === 'httpupgrade' || transport === 'http' || transport === 'h2') {
    return '/';
  }

  return '';
}

function getCredentialPart(server: ServerConfig): string {
  return normalizePart(
    server.uuid ||
    server.password ||
    server.peerPublicKey ||
    server.publicKey ||
    server.rawLink
  );
}

export function getServerIdentityKey(server: ServerConfig): string {
  return [
    getServerSelectionKey(server),
    normalizePart(server.name),
  ].join('|');
}

export function getServerSelectionKey(server: ServerConfig): string {
  return [
    normalizePart(server.protocol),
    normalizePart(server.address),
    normalizePart(server.port),
    getCredentialPart(server),
    normalizePart(server.transport),
    normalizePart(server.security),
    normalizePart(server.host),
    normalizeTransportPath(server),
    normalizePart(server.sni),
    normalizePart(server.flow),
  ].join('|');
}

export function findServerBySelectionKey(
  selectionKey: string | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  if (!selectionKey) return null;
  return servers.find((server) => getServerSelectionKey(server) === selectionKey) || null;
}

export function findMatchingServer(
  target: ServerConfig | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  if (!target) return null;

  const byId = servers.find((server) => server.id === target.id);
  if (byId) return byId;

  if (target.rawLink) {
    const byRawLink = servers.find((server) => server.rawLink && server.rawLink === target.rawLink);
    if (byRawLink) return byRawLink;
  }

  const targetIdentityKey = getServerIdentityKey(target);
  const byIdentityInSameSubscription = servers.find((server) =>
    server.subscriptionId === target.subscriptionId &&
    getServerIdentityKey(server) === targetIdentityKey
  );
  if (byIdentityInSameSubscription) return byIdentityInSameSubscription;

  const byIdentity = servers.find((server) => getServerIdentityKey(server) === targetIdentityKey);
  if (byIdentity) return byIdentity;

  const targetSelectionKey = getServerSelectionKey(target);
  const bySelectionInSameSubscription = servers.find((server) =>
    server.subscriptionId === target.subscriptionId &&
    getServerSelectionKey(server) === targetSelectionKey
  );
  if (bySelectionInSameSubscription) return bySelectionInSameSubscription;

  return findServerBySelectionKey(targetSelectionKey, servers);
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
