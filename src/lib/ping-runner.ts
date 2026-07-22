import type { ServerConfig, ServerPingUpdate } from '../stores/app-store';
import { pingServerSmart } from './utils';

const DEFAULT_PING_CONCURRENCY = 4;

function isUnsafeProbeHost(rawHost: string): boolean {
  const host = rawHost.trim().replace(/^\[|\]$/g, '').toLowerCase();
  if (!host || host === 'localhost' || host.endsWith('.localhost') || host.endsWith('.local')) return true;

  const ipv4 = host.split('.').map(Number);
  if (ipv4.length === 4 && ipv4.every((part) => Number.isInteger(part) && part >= 0 && part <= 255)) {
    const [a, b] = ipv4;
    return a === 0 || a === 10 || a === 127 || a >= 224 ||
      (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) ||
      (a === 192 && b === 168);
  }

  return host === '::' || host === '::1' || host.startsWith('fc') || host.startsWith('fd') || host.startsWith('fe8') || host.startsWith('fe9') || host.startsWith('fea') || host.startsWith('feb');
}

type InvokeFn = (cmd: string, args: any) => Promise<any>;

interface PingServersOptions {
  concurrency?: number;
  isCancelled?: () => boolean;
  onActiveIdsChange?: (ids: Set<string>) => void;
  onBatch: (updates: ServerPingUpdate[]) => void;
}

export async function pingServersWithLimit(
  servers: ServerConfig[],
  invoke: InvokeFn,
  {
    concurrency = DEFAULT_PING_CONCURRENCY,
    isCancelled = () => false,
    onActiveIdsChange,
    onBatch,
  }: PingServersOptions,
): Promise<void> {
  if (servers.length === 0) return;

  const activeIds = new Set<string>();
  const pendingUpdates: ServerPingUpdate[] = [];
  let nextIndex = 0;

  const emitActiveIds = () => {
    onActiveIdsChange?.(new Set(activeIds));
  };

  const flush = () => {
    if (pendingUpdates.length === 0 || isCancelled()) return;
    onBatch(pendingUpdates.splice(0));
  };

  const worker = async () => {
    while (!isCancelled()) {
      const server = servers[nextIndex];
      nextIndex += 1;
      if (!server) break;

      activeIds.add(server.id);
      emitActiveIds();

      try {
        const ping = isUnsafeProbeHost(server.address) ? -1 : await pingServerSmart(server, invoke);
        if (!isCancelled()) pendingUpdates.push({ id: server.id, ping });
      } catch {
        if (!isCancelled()) pendingUpdates.push({ id: server.id, ping: -1 });
      } finally {
        activeIds.delete(server.id);
        emitActiveIds();
        if (pendingUpdates.length >= concurrency) flush();
      }
    }
  };

  const workerCount = Math.min(Math.max(1, concurrency), servers.length);
  await Promise.all(Array.from({ length: workerCount }, worker));
  flush();
}
