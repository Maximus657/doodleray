import type { ServerConfig } from '../stores/app-store';

// ========== VLESS Parser ==========

export function parseVlessLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('vless://')) return null;

    const url = new URL(link.replace('vless://', 'https://'));
    const uuid = url.username;
    const address = url.hostname;
    const port = parseInt(url.port || '443');
    const params = url.searchParams;
    const name = decodeURIComponent(url.hash.slice(1) || `${address}:${port}`);

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'vless',
      address,
      port,
      uuid,
      transport: params.get('type') || 'tcp',
      security: params.get('security') || 'none',
      host: params.get('host') || undefined,
      path: params.get('path') || undefined,
      sni: params.get('sni') || undefined,
      fingerprint: params.get('fp') || undefined,
      publicKey: params.get('pbk') || undefined,
      shortId: params.get('sid') || undefined,
      flow: params.get('flow') || undefined,
      encryption: params.get('encryption') || 'none',
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== VMess Parser ==========

interface VMessConfig {
  v?: string;
  ps?: string;
  add?: string;
  port?: number | string;
  id?: string;
  aid?: number | string;
  net?: string;
  type?: string;
  host?: string;
  path?: string;
  tls?: string;
  sni?: string;
  fp?: string;
}

export function parseVmessLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('vmess://')) return null;

    const base64 = link.replace('vmess://', '');
    const json: VMessConfig = JSON.parse(atob(base64));

    return {
      id: crypto.randomUUID(),
      name: json.ps || `${json.add}:${json.port}`,
      protocol: 'vmess',
      address: json.add || '',
      port: parseInt(String(json.port || '443')),
      uuid: json.id,
      transport: json.net || 'tcp',
      security: json.tls || 'none',
      host: json.host || undefined,
      path: json.path || undefined,
      sni: json.sni || undefined,
      fingerprint: json.fp || undefined,
      encryption: 'auto',
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== Trojan Parser ==========

export function parseTrojanLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('trojan://')) return null;

    const url = new URL(link.replace('trojan://', 'https://'));
    const password = url.username;
    const address = url.hostname;
    const port = parseInt(url.port || '443');
    const params = url.searchParams;
    const name = decodeURIComponent(url.hash.slice(1) || `${address}:${port}`);

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'trojan',
      address,
      port,
      password,
      transport: params.get('type') || 'tcp',
      security: params.get('security') || 'tls',
      host: params.get('host') || undefined,
      path: params.get('path') || undefined,
      sni: params.get('sni') || undefined,
      fingerprint: params.get('fp') || undefined,
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== Shadowsocks Parser ==========

export function parseShadowsocksLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('ss://')) return null;

    // Format: ss://BASE64(method:password)@server:port#name
    // or ss://BASE64(method:password@server:port)#name
    const withoutPrefix = link.replace('ss://', '');
    const hashIndex = withoutPrefix.indexOf('#');
    const namePart = hashIndex >= 0 ? decodeURIComponent(withoutPrefix.slice(hashIndex + 1)) : '';
    const mainPart = hashIndex >= 0 ? withoutPrefix.slice(0, hashIndex) : withoutPrefix;

    let address: string, port: number, encryption: string, password: string;

    if (mainPart.includes('@')) {
      // Format: BASE64(method:password)@server:port
      const [encodedPart, serverPart] = mainPart.split('@');
      const decoded = atob(encodedPart);
      const [method, pass] = decoded.split(':');
      encryption = method;
      password = pass;
      const [addr, p] = serverPart.split(':');
      address = addr;
      port = parseInt(p);
    } else {
      // Format: BASE64(method:password@server:port)
      const decoded = atob(mainPart);
      const [methodPass, serverPort] = decoded.split('@');
      const [method, pass] = methodPass.split(':');
      const [addr, p] = serverPort.split(':');
      encryption = method;
      password = pass;
      address = addr;
      port = parseInt(p);
    }

    return {
      id: crypto.randomUUID(),
      name: namePart || `${address}:${port}`,
      protocol: 'shadowsocks',
      address,
      port,
      password,
      transport: 'tcp',
      security: 'none',
      encryption,
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== Hysteria2 Parser ==========

export function parseHysteria2Link(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('hy2://') && !link.startsWith('hysteria2://')) return null;

    const normalized = link.replace('hysteria2://', 'https://').replace('hy2://', 'https://');
    const url = new URL(normalized);
    const password = decodeURIComponent(url.username);
    const address = url.hostname;
    const port = parseInt(url.port || '443');
    const params = url.searchParams;
    const name = decodeURIComponent(url.hash.slice(1) || `Hysteria2 ${address}:${port}`);
    const country = detectCountry(name);

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'hysteria2',
      address,
      port,
      password,
      transport: 'udp',
      security: 'tls',
      sni: params.get('sni') || undefined,
      fingerprint: params.get('pinSHA256') || undefined,
      obfsType: params.get('obfs') || undefined,
      obfsPassword: params.get('obfs-password') || undefined,
      upMbps: params.get('up') ? parseInt(params.get('up')!) : undefined,
      downMbps: params.get('down') ? parseInt(params.get('down')!) : undefined,
      country: country?.name,
      countryCode: country?.code,
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== TUIC Parser ==========

export function parseTuicLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('tuic://')) return null;

    // Format: tuic://uuid:password@server:port?params#name
    const url = new URL(link.replace('tuic://', 'https://'));
    const uuid = decodeURIComponent(url.username);
    const password = decodeURIComponent(url.password);
    const address = url.hostname;
    const port = parseInt(url.port || '443');
    const params = url.searchParams;
    const name = decodeURIComponent(url.hash.slice(1) || `TUIC ${address}:${port}`);
    const country = detectCountry(name);

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'tuic',
      address,
      port,
      uuid,
      password,
      transport: 'quic',
      security: 'tls',
      sni: params.get('sni') || undefined,
      congestionControl: params.get('congestion_control') || 'bbr',
      udpRelayMode: params.get('udp_relay_mode') || 'native',
      alpn: params.get('alpn')?.split(',') || ['h3'],
      country: country?.name,
      countryCode: country?.code,
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== WireGuard Parser ==========

export function parseWireGuardLink(link: string): ServerConfig | null {
  try {
    if (!link.startsWith('wireguard://') && !link.startsWith('wg://')) return null;

    const normalized = link.replace('wireguard://', 'https://').replace('wg://', 'https://');
    const url = new URL(normalized);
    const address = url.hostname;
    const port = parseInt(url.port || '51820');
    const params = url.searchParams;
    const name = decodeURIComponent(url.hash.slice(1) || `WireGuard ${address}:${port}`);
    const country = detectCountry(name);

    const reservedStr = params.get('reserved');
    let reserved: number[] | undefined;
    if (reservedStr) {
      reserved = reservedStr.split(',').map(Number);
    }

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'wireguard',
      address,
      port,
      transport: 'udp',
      security: 'none',
      privateKey: params.get('privatekey') || params.get('private_key') || undefined,
      peerPublicKey: params.get('publickey') || params.get('public_key') || undefined,
      preSharedKey: params.get('presharedkey') || params.get('pre_shared_key') || undefined,
      localAddress: params.get('address')?.split(',') || ['10.0.0.2/32'],
      reserved,
      mtu: params.get('mtu') ? parseInt(params.get('mtu')!) : 1408,
      workers: params.get('workers') ? parseInt(params.get('workers')!) : undefined,
      country: country?.name,
      countryCode: country?.code,
      rawLink: link,
    };
  } catch {
    return null;
  }
}

// ========== Universal Parser ==========

export function parseProxyLink(link: string): ServerConfig | null {
  const trimmed = link.trim();
  if (trimmed.startsWith('vless://')) return parseVlessLink(trimmed);
  if (trimmed.startsWith('vmess://')) return parseVmessLink(trimmed);
  if (trimmed.startsWith('trojan://')) return parseTrojanLink(trimmed);
  if (trimmed.startsWith('ss://')) return parseShadowsocksLink(trimmed);
  if (trimmed.startsWith('hy2://') || trimmed.startsWith('hysteria2://')) return parseHysteria2Link(trimmed);
  if (trimmed.startsWith('tuic://')) return parseTuicLink(trimmed);
  if (trimmed.startsWith('wireguard://') || trimmed.startsWith('wg://')) return parseWireGuardLink(trimmed);
  return null;
}

// ========== Batch Parser (for subscriptions) ==========

export function parseMultipleLinks(text: string): ServerConfig[] {
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  const servers: ServerConfig[] = [];

  for (const line of lines) {
    const server = parseProxyLink(line);
    if (server) servers.push(server);
  }

  return servers;
}

// ========== Country Detection ==========

// Emoji flag → country code (flag is two regional indicator chars)
function extractFlagEmoji(text: string): string | undefined {
  // Regional indicator symbols: U+1F1E6 to U+1F1FF
  const flagRegex = /[\u{1F1E6}-\u{1F1FF}]{2}/gu;
  const match = text.match(flagRegex);
  if (!match) return undefined;
  const codePoints = [...match[0]];
  const code = codePoints
    .map((cp) => String.fromCharCode(cp.codePointAt(0)! - 0x1f1e6 + 65))
    .join('');
  return code;
}

const COUNTRY_NAMES: Record<string, { name: string; code: string }> = {
  amsterdam: { name: 'Netherlands', code: 'NL' },
  netherlands: { name: 'Netherlands', code: 'NL' },
  germany: { name: 'Germany', code: 'DE' },
  frankfurt: { name: 'Germany', code: 'DE' },
  berlin: { name: 'Germany', code: 'DE' },
  russia: { name: 'Russia', code: 'RU' },
  moscow: { name: 'Russia', code: 'RU' },
  'united states': { name: 'United States', code: 'US' },
  'los angeles': { name: 'United States', code: 'US' },
  'new york': { name: 'United States', code: 'US' },
  singapore: { name: 'Singapore', code: 'SG' },
  japan: { name: 'Japan', code: 'JP' },
  tokyo: { name: 'Japan', code: 'JP' },
  'hong kong': { name: 'Hong Kong', code: 'HK' },
  london: { name: 'United Kingdom', code: 'GB' },
  france: { name: 'France', code: 'FR' },
  paris: { name: 'France', code: 'FR' },
  finland: { name: 'Finland', code: 'FI' },
  helsinki: { name: 'Finland', code: 'FI' },
  canada: { name: 'Canada', code: 'CA' },
  toronto: { name: 'Canada', code: 'CA' },
  australia: { name: 'Australia', code: 'AU' },
  sydney: { name: 'Australia', code: 'AU' },
  turkey: { name: 'Turkey', code: 'TR' },
  istanbul: { name: 'Turkey', code: 'TR' },
  korea: { name: 'South Korea', code: 'KR' },
  seoul: { name: 'South Korea', code: 'KR' },
  india: { name: 'India', code: 'IN' },
  mumbai: { name: 'India', code: 'IN' },
  brazil: { name: 'Brazil', code: 'BR' },
};

export function detectCountry(serverName: string): { name: string; code: string } | undefined {
  // 1. Try emoji flag first (most reliable)
  const flagCode = extractFlagEmoji(serverName);
  if (flagCode) {
    // Map common codes to names
    const flagMap: Record<string, string> = {
      NL: 'Netherlands', DE: 'Germany', RU: 'Russia', US: 'United States',
      SG: 'Singapore', JP: 'Japan', HK: 'Hong Kong', GB: 'United Kingdom',
      FR: 'France', FI: 'Finland', CA: 'Canada', AU: 'Australia',
      TR: 'Turkey', KR: 'South Korea', IN: 'India', BR: 'Brazil',
    };
    return { name: flagMap[flagCode] || flagCode, code: flagCode };
  }

  // 2. Match by city/country name
  const lower = serverName.toLowerCase();
  for (const [keyword, value] of Object.entries(COUNTRY_NAMES)) {
    if (lower.includes(keyword)) return value;
  }

  return undefined;
}

