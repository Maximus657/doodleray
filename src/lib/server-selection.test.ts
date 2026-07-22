import { getServerSelectionKey, resolveConnectServer, selectPreferredServer } from './server-selection.ts';
import type { ServerConfig } from '../stores/app-store.ts';

function server(overrides: Partial<ServerConfig>): ServerConfig {
  return {
    id: 'server',
    name: 'Germany',
    protocol: 'vless',
    address: 'Example.COM',
    port: 443,
    password: 'CaseSensitiveSecret',
    transport: 'ws',
    security: 'tls',
    host: 'CDN.Example.COM',
    path: '/CaseSensitivePath',
    rawLink: 'vless://example',
    ...overrides,
  };
}

function assertEqual(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

function assertNotEqual(actual: unknown, expected: unknown) {
  if (actual === expected) throw new Error(`Expected values to differ, got ${String(actual)}`);
}

assertEqual(
  getServerSelectionKey(server({ address: 'EXAMPLE.com', host: 'cdn.example.com' })),
  getServerSelectionKey(server({ address: 'example.COM', host: 'CDN.EXAMPLE.COM' })),
);
assertNotEqual(
  getServerSelectionKey(server({ password: 'CaseSensitiveSecret' })),
  getServerSelectionKey(server({ password: 'casesensitivesecret' })),
);
assertNotEqual(
  getServerSelectionKey(server({ path: '/CaseSensitivePath' })),
  getServerSelectionKey(server({ path: '/casesensitivepath' })),
);

const russia = server({ id: 'app-location:ru', name: 'Россия', country: 'Россия', countryCode: 'RU', ping: 5 });
const germany = server({ id: 'app-location:de', name: 'Германия', country: 'Германия', countryCode: 'DE', ping: 30 });
assertEqual(selectPreferredServer([russia, germany], true)?.id, germany.id);
assertEqual(selectPreferredServer([russia], true), null);
assertEqual(resolveConnectServer(russia, [russia, germany], true)?.id, russia.id);
