import type { ServerConfig, ServerPingUpdate } from '../stores/app-store';
import { pingServerSmart } from './utils';

const DEFAULT_PING_CONCURRENCY = 4;

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
        const ping = await pingServerSmart(server, invoke);
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
