import {
  findLegacyDoodleSubscriptionUrl,
  findLegacyDoodleSubscriptionUrls,
  isLegacyDoodleSubscriptionUrl,
  mergeLegacyDoodleSubscriptionState,
} from './legacy-subscription.ts';

function assertEqual(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

const canonical = 'https://ddlvpn.lol/s/oldDesktopToken123';
assertEqual(findLegacyDoodleSubscriptionUrl([
  { url: 'app://doodlevpn' },
  { url: canonical },
]), canonical);

assertEqual(findLegacyDoodleSubscriptionUrl([
  { url: 'https://example.com/s/not-a-doodle-token' },
  { url: 'vless://opaque' },
]), null);

assertEqual(
  findLegacyDoodleSubscriptionUrl([{ url: 'https://doodlevpn.online/sub/legacy_token-456?format=happ' }]),
  'https://doodlevpn.online/sub/legacy_token-456?format=happ',
);

const secondary = 'https://doodlevpn.online/sub/secondDesktopToken456';
assertEqual(
  JSON.stringify(findLegacyDoodleSubscriptionUrls([
    { url: 'https://third-party.example/sub/external-token' },
    { url: canonical },
    { url: 'vless://manual-server' },
    { url: secondary },
    { url: canonical },
  ])),
  JSON.stringify([canonical, secondary]),
);
assertEqual(isLegacyDoodleSubscriptionUrl('https://example.com/s/not-doodle'), false);
assertEqual(isLegacyDoodleSubscriptionUrl(canonical), true);
assertEqual(
  JSON.stringify(findLegacyDoodleSubscriptionUrls([
    { id: 'older', url: canonical },
    { id: 'active', url: secondary },
  ], 'active')),
  JSON.stringify([secondary, canonical]),
);

const secureSnapshot = JSON.stringify({
  state: { subscriptions: [], activeServer: null, theme: 'light' },
  version: 0,
});
const legacySnapshot = JSON.stringify({
  state: {
    subscriptions: [
      { id: 'stale', url: canonical },
      { id: 'active', url: secondary },
      { id: 'external', url: 'https://example.com/sub/external-token' },
    ],
    activeServer: { id: 'server-2', subscriptionId: 'active' },
    theme: 'dark',
  },
  version: 0,
});
const reconciled = JSON.parse(
  mergeLegacyDoodleSubscriptionState(secureSnapshot, legacySnapshot),
);
assertEqual(reconciled.state.theme, 'light');
assertEqual(reconciled.state.subscriptions.length, 2);
assertEqual(reconciled.state.subscriptions[0].url, canonical);
assertEqual(reconciled.state.subscriptions[1].url, secondary);
assertEqual(reconciled.state.activeServer.subscriptionId, 'active');
assertEqual(
  mergeLegacyDoodleSubscriptionState(secureSnapshot, '{invalid'),
  secureSnapshot,
);

console.log('app control-plane migration tests passed');
