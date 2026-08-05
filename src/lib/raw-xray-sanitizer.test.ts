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

const xhttpStream = {
  network: 'xhttp',
  security: 'tls',
  tlsSettings: { serverName: 'edge.example.test', fingerprint: 'firefox', alpn: ['h2'] },
  xhttpSettings: {
    host: 'edge.example.test',
    path: '/opaque',
    mode: 'packet-up',
    extra: {
      uplinkHTTPMethod: 'POST',
      scMaxEachPostBytes: 131072,
      scMinPostsIntervalMs: 30,
      xPaddingBytes: '100-1000',
      xmux: { maxConcurrency: 4, maxConnections: 0, cMaxReuseTimes: 8, hKeepAlivePeriod: 30, hMaxReusableSecs: 600 },
    },
  },
};
const xhttp = sanitizeRawXrayConfig({
  outbounds: [{ tag: 'xhttp', protocol: 'vless', settings: { vnext: [{ address: 'vpn.example', port: 443 }] }, streamSettings: xhttpStream }],
}, { protocol: 'vless', address: 'vpn.example', port: 443 });
const xhttpOutbound = (xhttp?.outbounds as Array<{ tag?: string; streamSettings?: unknown }>).find((outbound) => outbound.tag === 'doodleray-selected-proxy');
if (JSON.stringify(xhttpOutbound?.streamSettings) !== JSON.stringify(xhttpStream)) {
  throw new Error('Imported XHTTP TLS and packet-up settings must survive sanitization');
}
