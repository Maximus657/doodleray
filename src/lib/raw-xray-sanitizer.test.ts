import { sanitizeRawXrayConfig } from './raw-xray-sanitizer.ts';

const sanitized = sanitizeRawXrayConfig({
  dns: {
    servers: ['http://169.254.169.254/latest/meta-data'],
  },
  log: { access: '/tmp/owned', error: '/tmp/owned-error' },
  routing: { rules: [{ type: 'field', network: 'tcp,udp', outboundTag: 'direct' }] },
  outbounds: [
    { tag: 'direct', protocol: 'freedom' },
    { tag: 'proxy', protocol: 'vless', settings: { vnext: [{ address: 'vpn.example', port: 443 }] } },
  ],
}, { protocol: 'vless', address: 'vpn.example', port: 443 });

if (!sanitized || 'log' in sanitized) throw new Error('Imported Xray log paths survived sanitization');
if ('dns' in sanitized) throw new Error('Imported Xray DNS survived sanitization');
const routing = sanitized.routing as { rules: Array<{ outboundTag: string }> };
if (routing.rules.length !== 1 || routing.rules[0]?.outboundTag !== 'doodleray-selected-proxy') {
  throw new Error('Imported Xray routing was not replaced with the selected proxy route');
}
