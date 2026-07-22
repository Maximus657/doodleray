import { findLegacyDoodleSubscriptionUrl } from './legacy-subscription.ts';

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

console.log('app control-plane migration tests passed');
