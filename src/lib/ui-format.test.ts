import { antiJammerQuotaView, displayLocationTitle } from './ui-format.ts';

function assert(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

assert(displayLocationTitle('🇩🇪 DE · VLESS Reality · Germany', 'DE'), 'Germany');
assert(displayLocationTitle('Hysteria2 / Germany', 'DE'), 'Germany');
assert(displayLocationTitle('VLESS Reality', 'NL'), 'NL');
assert(antiJammerQuotaView({ limitBytes: 100, remainingBytes: 21, lowBalance: false, exhausted: false }).tone, 'normal');
assert(antiJammerQuotaView({ limitBytes: 100, remainingBytes: 20, lowBalance: false, exhausted: false }).tone, 'low');
assert(antiJammerQuotaView({ limitBytes: 100, remainingBytes: 40, lowBalance: false, exhausted: true }).tone, 'exhausted');
