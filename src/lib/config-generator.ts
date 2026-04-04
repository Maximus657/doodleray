import type { ServerConfig } from '../stores/app-store';

// ========== Sing-box Config Generator ==========
// Generates sing-box JSON config for all supported protocols:
// VLESS, VMess, Trojan, Shadowsocks, Hysteria2, TUIC, WireGuard

export interface SingboxConfig {
  log: { level: string };
  dns: Record<string, unknown>;
  inbounds: Record<string, unknown>[];
  outbounds: Record<string, unknown>[];
  route: Record<string, unknown>;
}

export function generateSingboxConfig(
  server: ServerConfig,
  options: {
    socksPort: number;
    httpPort: number;
    proxyMode: 'system-proxy' | 'tun' | 'vpn';
    networkStack: 'mixed' | 'system' | 'gvisor';
    dnsMode: 'fakeip' | 'realip';
    strictRoute: boolean;
  }
): SingboxConfig {
  const { socksPort, httpPort, proxyMode, networkStack, dnsMode, strictRoute } = options;

  return {
    log: { level: 'info' },
    dns: generateDns(dnsMode),
    inbounds: generateInbounds(proxyMode, socksPort, httpPort, networkStack, strictRoute),
    outbounds: [
      generateOutbound(server),
      { type: 'direct', tag: 'direct' },
      { type: 'dns', tag: 'dns-out' },
      { type: 'block', tag: 'block' },
    ],
    route: {
      auto_detect_interface: true,
      rules: [
        { protocol: 'dns', outbound: 'dns-out' },
      ],
    },
  };
}

function generateDns(mode: string): Record<string, unknown> {
  if (mode === 'fakeip') {
    return {
      servers: [
        { tag: 'dns-remote', address: 'https://1.1.1.1/dns-query', detour: 'proxy' },
        { tag: 'dns-fakeip', address: 'fakeip' },
        { tag: 'dns-direct', address: 'https://223.5.5.5/dns-query', detour: 'direct' },
      ],
      rules: [{ query_type: ['A', 'AAAA'], server: 'dns-fakeip' }],
      fakeip: { enabled: true, inet4_range: '198.18.0.0/15', inet6_range: 'fc00::/18' },
      independent_cache: true,
    };
  }
  return {
    servers: [
      { tag: 'dns-remote', address: 'https://1.1.1.1/dns-query', detour: 'proxy' },
      { tag: 'dns-direct', address: 'https://223.5.5.5/dns-query', detour: 'direct' },
    ],
    independent_cache: true,
  };
}

function generateInbounds(
  mode: string,
  socksPort: number,
  httpPort: number,
  networkStack: string,
  strictRoute: boolean
): Record<string, unknown>[] {
  if (mode === 'tun' || mode === 'vpn') {
    return [{
      type: 'tun',
      tag: 'tun-in',
      inet4_address: '172.19.0.1/30',
      inet6_address: 'fdfe:dcba:9876::1/126',
      auto_route: true,
      strict_route: strictRoute,
      stack: networkStack,
      sniff: true,
    }];
  }
  return [
    { type: 'socks', tag: 'socks-in', listen: '127.0.0.1', listen_port: socksPort, sniff: true },
    { type: 'http', tag: 'http-in', listen: '127.0.0.1', listen_port: httpPort, sniff: true },
  ];
}

function generateOutbound(server: ServerConfig): Record<string, unknown> {
  switch (server.protocol) {
    case 'vless': return generateVlessOutbound(server);
    case 'vmess': return generateVmessOutbound(server);
    case 'trojan': return generateTrojanOutbound(server);
    case 'shadowsocks': return generateShadowsocksOutbound(server);
    case 'hysteria2': return generateHysteria2Outbound(server);
    case 'tuic': return generateTuicOutbound(server);
    case 'wireguard': return generateWireGuardOutbound(server);
    default: return { type: 'direct', tag: 'proxy' };
  }
}

// ========== Protocol-specific outbounds ==========

function generateVlessOutbound(server: ServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {
    type: 'vless',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    uuid: server.uuid || '',
    flow: server.flow || '',
  };

  // TLS
  if (server.security === 'tls' || server.security === 'reality') {
    out.tls = {
      enabled: true,
      server_name: server.sni || server.address,
      utls: { enabled: true, fingerprint: server.fingerprint || 'chrome' },
      ...(server.security === 'reality' ? {
        reality: {
          enabled: true,
          public_key: server.publicKey || '',
          short_id: server.shortId || '',
        }
      } : {}),
    };
  }

  // Transport
  if (server.transport === 'ws') {
    out.transport = {
      type: 'ws',
      path: server.path || '/',
      headers: { Host: server.host || server.address },
    };
  } else if (server.transport === 'grpc') {
    out.transport = { type: 'grpc', service_name: server.path || '' };
  } else if (server.transport === 'httpupgrade') {
    out.transport = { type: 'httpupgrade', path: server.path || '/', host: server.host || server.address };
  }

  return out;
}

function generateVmessOutbound(server: ServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {
    type: 'vmess',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    uuid: server.uuid || '',
    security: server.encryption || 'auto',
  };

  if (server.security === 'tls') {
    out.tls = { enabled: true, server_name: server.sni || server.address };
  }

  if (server.transport === 'ws') {
    out.transport = { type: 'ws', path: server.path || '/' };
  }

  return out;
}

function generateTrojanOutbound(server: ServerConfig): Record<string, unknown> {
  return {
    type: 'trojan',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    password: server.password || '',
    tls: { enabled: true, server_name: server.sni || server.address },
  };
}

function generateShadowsocksOutbound(server: ServerConfig): Record<string, unknown> {
  return {
    type: 'shadowsocks',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    password: server.password || '',
    method: server.encryption || 'aes-256-gcm',
  };
}

// ========== NEW PROTOCOLS ==========

function generateHysteria2Outbound(server: ServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {
    type: 'hysteria2',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    password: server.password || '',
    tls: {
      enabled: true,
      server_name: server.sni || server.address,
    },
  };

  // Obfuscation (Salamander)
  if (server.obfsType) {
    out.obfs = {
      type: server.obfsType,
      password: server.obfsPassword || '',
    };
  }

  // Bandwidth limits
  if (server.upMbps) out.up_mbps = server.upMbps;
  if (server.downMbps) out.down_mbps = server.downMbps;

  return out;
}

function generateTuicOutbound(server: ServerConfig): Record<string, unknown> {
  return {
    type: 'tuic',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    uuid: server.uuid || '',
    password: server.password || '',
    congestion_control: server.congestionControl || 'bbr',
    udp_relay_mode: server.udpRelayMode || 'native',
    tls: {
      enabled: true,
      server_name: server.sni || server.address,
      alpn: server.alpn || ['h3'],
    },
  };
}

function generateWireGuardOutbound(server: ServerConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {
    type: 'wireguard',
    tag: 'proxy',
    server: server.address,
    server_port: server.port,
    private_key: server.privateKey || '',
    peer_public_key: server.peerPublicKey || '',
    local_address: server.localAddress || ['10.0.0.2/32'],
    mtu: server.mtu || 1408,
  };

  if (server.preSharedKey) out.pre_shared_key = server.preSharedKey;
  if (server.reserved) out.reserved = server.reserved;
  if (server.workers) out.workers = server.workers;

  return out;
}

// ========== Export config as JSON string ==========
export function configToJson(config: SingboxConfig): string {
  return JSON.stringify(config, null, 2);
}
