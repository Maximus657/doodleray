import {
  findLegacyDoodleSubscriptionUrl,
  findLegacyDoodleSubscriptionUrls,
  isLegacyDoodleSubscriptionUrl,
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

console.log('app control-plane migration tests passed');
