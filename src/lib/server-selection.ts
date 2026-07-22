import type { ServerConfig } from '../stores/app-store';

function normalizeCaseInsensitivePart(value: unknown): string {
  return String(value ?? '').trim().toLowerCase();
}

function normalizeOpaquePart(value: unknown): string {
  return String(value ?? '').trim();
}

function normalizeTransportPath(server: ServerConfig): string {
  const path = normalizeOpaquePart(server.path);
  if (path) return path;

  const transport = normalizeCaseInsensitivePart(server.transport);
  if (transport === 'ws' || transport === 'httpupgrade' || transport === 'http' || transport === 'h2') {
    return '/';
  }

  return '';
}

function getCredentialPart(server: ServerConfig): string {
  if (server.uuid) return normalizeCaseInsensitivePart(server.uuid);
  return normalizeOpaquePart(
    server.password || server.peerPublicKey || server.publicKey || server.rawLink
  );
}

function scopedKey(subscriptionId: string | undefined, key: string): string {
  return `${subscriptionId ?? ''}\u0000${key}`;
}

function setFirst(map: Map<string, ServerConfig>, key: string | undefined, server: ServerConfig) {
  if (!key || map.has(key)) return;
  map.set(key, server);
}

export interface ServerSelectionIndex {
  byId: Map<string, ServerConfig>;
  byRawLink: Map<string, ServerConfig>;
  byIdentity: Map<string, ServerConfig>;
  bySubscriptionIdentity: Map<string, ServerConfig>;
  bySelection: Map<string, ServerConfig>;
  bySubscriptionSelection: Map<string, ServerConfig>;
}

export function getServerIdentityKey(server: ServerConfig): string {
  return [
    getServerSelectionKey(server),
    normalizeCaseInsensitivePart(server.name),
  ].join('|');
}

export function getServerSelectionKey(server: ServerConfig): string {
  return [
    normalizeCaseInsensitivePart(server.protocol),
    normalizeCaseInsensitivePart(server.address),
    normalizeCaseInsensitivePart(server.port),
    getCredentialPart(server),
    normalizeCaseInsensitivePart(server.transport),
    normalizeCaseInsensitivePart(server.security),
    normalizeCaseInsensitivePart(server.host),
    normalizeTransportPath(server),
    normalizeCaseInsensitivePart(server.sni),
    normalizeCaseInsensitivePart(server.flow),
  ].join('|');
}

/// Deterministic profile id: subscription id + normalized outbound identity
/// (+ name as tiebreaker inside getServerIdentityKey). Stable across
/// refreshes so selection, ping maps, and React keys survive; old persisted
/// selections with random ids still migrate through the identity index.
export function stableServerId(subscriptionId: string, server: ServerConfig): string {
  const key = `${subscriptionId}|${getServerIdentityKey(server)}`;
  let h1 = 5381;
  let h2 = 52711;
  for (let i = 0; i < key.length; i++) {
    const code = key.charCodeAt(i);
    h1 = ((h1 * 33) ^ code) >>> 0;
    h2 = ((h2 * 31) ^ code) >>> 0;
  }
  return `sub-${h1.toString(16).padStart(8, '0')}${h2.toString(16).padStart(8, '0')}`;
}

export function buildServerSelectionIndex(servers: ServerConfig[]): ServerSelectionIndex {
  const index: ServerSelectionIndex = {
    byId: new Map(),
    byRawLink: new Map(),
    byIdentity: new Map(),
    bySubscriptionIdentity: new Map(),
    bySelection: new Map(),
    bySubscriptionSelection: new Map(),
  };

  for (const server of servers) {
    const identityKey = getServerIdentityKey(server);
    const selectionKey = getServerSelectionKey(server);

    setFirst(index.byId, server.id, server);
    setFirst(index.byRawLink, server.rawLink, server);
    setFirst(index.byIdentity, identityKey, server);
    setFirst(index.bySubscriptionIdentity, scopedKey(server.subscriptionId, identityKey), server);
    setFirst(index.bySelection, selectionKey, server);
    setFirst(index.bySubscriptionSelection, scopedKey(server.subscriptionId, selectionKey), server);
  }

  return index;
}

export function findServerBySelectionKey(
  selectionKey: string | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  if (!selectionKey) return null;
  return buildServerSelectionIndex(servers).bySelection.get(selectionKey) || null;
}

export function findServerBySelectionKeyInIndex(
  selectionKey: string | null | undefined,
  index: ServerSelectionIndex,
): ServerConfig | null {
  if (!selectionKey) return null;
  return index.bySelection.get(selectionKey) || null;
}

export function findMatchingServerInIndex(
  target: ServerConfig | null | undefined,
  index: ServerSelectionIndex,
): ServerConfig | null {
  if (!target) return null;

  const byId = index.byId.get(target.id);
  if (byId) return byId;

  if (target.rawLink) {
    const byRawLink = index.byRawLink.get(target.rawLink);
    if (byRawLink) return byRawLink;
  }

  const targetIdentityKey = getServerIdentityKey(target);
  const byIdentityInSameSubscription = index.bySubscriptionIdentity.get(
    scopedKey(target.subscriptionId, targetIdentityKey)
  );
  if (byIdentityInSameSubscription) return byIdentityInSameSubscription;

  const byIdentity = index.byIdentity.get(targetIdentityKey);
  if (byIdentity) return byIdentity;

  const targetSelectionKey = getServerSelectionKey(target);
  const bySelectionInSameSubscription = index.bySubscriptionSelection.get(
    scopedKey(target.subscriptionId, targetSelectionKey)
  );
  if (bySelectionInSameSubscription) return bySelectionInSameSubscription;

  return findServerBySelectionKeyInIndex(targetSelectionKey, index);
}

export function findMatchingServer(
  target: ServerConfig | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  return findMatchingServerInIndex(target, buildServerSelectionIndex(servers));
}

export function selectPreferredServer(
  servers: ServerConfig[],
  autoSelectFastest: boolean,
): ServerConfig | null {
  if (servers.length === 0) return null;
  if (!autoSelectFastest) return servers[0];

  let fastest: ServerConfig | null = null;
  for (const server of servers) {
    if (server.ping !== undefined && server.ping > 0 && (!fastest || server.ping < fastest.ping!)) {
      fastest = server;
    }
  }

  return fastest || servers[0];
}

export function resolveConnectServer(
  activeServer: ServerConfig | null,
  servers: ServerConfig[],
  autoSelectFastest: boolean,
): ServerConfig | null {
  return findMatchingServer(activeServer, servers) || selectPreferredServer(servers, autoSelectFastest);
}
