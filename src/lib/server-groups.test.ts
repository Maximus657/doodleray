import { buildServerDisplayGroups } from './server-groups.ts';
import type { ServerConfig } from '../stores/app-store.ts';

function server(overrides: Partial<ServerConfig>): ServerConfig {
  return {
    id: 'server',
    name: '🇩🇪 Германия',
    protocol: 'vless',
    address: 'example.com',
    port: 443,
    transport: 'tcp',
    security: 'tls',
    rawLink: 'vless://example',
    subscriptionId: 'subscription',
    ...overrides,
  };
}

function assert(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

const groups = buildServerDisplayGroups([
  server({ id: 'slow', ping: 80 }),
  server({ id: 'fast', ping: 24 }),
]);

assert(groups.length, 1);
assert(groups[0]?.servers.length, 2);
assert(groups[0]?.selectedServer.id, 'fast');
assert(groups[0]?.ping, 24);

assert(buildServerDisplayGroups([
  server({ id: 'one', subscriptionId: 'one' }),
  server({ id: 'two', subscriptionId: 'two' }),
]).length, 2);
