import type { Subscription, ServerConfig } from '../stores/app-store';
import { parseMultipleLinks, detectCountry } from './parser';

// ========== Xray JSON Config Parser ==========

interface XrayJsonConfig {
  remarks?: string;
  outbounds?: Array<{
    tag?: string;
    protocol?: string;
    settings?: {
      vnext?: Array<{
        address?: string;
        port?: number;
        users?: Array<{
          id?: string;
          encryption?: string;
          flow?: string;
          security?: string;
        }>;
      }>;
      servers?: Array<{
        address?: string;
        port?: number;
        password?: string;
        method?: string;
      }>;
    };
    streamSettings?: {
      security?: string;
      network?: string;
      realitySettings?: {
        fingerprint?: string;
        publicKey?: string;
        serverName?: string;
        shortId?: string;
      };
      xhttpSettings?: {
        mode?: string;
        path?: string;
      };
      wsSettings?: {
        path?: string;
        headers?: { Host?: string };
      };
      tlsSettings?: {
        serverName?: string;
        fingerprint?: string;
      };
    };
  }>;
  routing?: {
    rules?: Array<{
      domain?: string[];
      outboundTag?: string;
      balancerTag?: string;
    }>;
  };
}

function parseXrayJsonConfig(json: XrayJsonConfig): ServerConfig | null {
  try {
    const name = json.remarks || 'Unknown Server';
    const proxyProtocols = ['vless', 'vmess', 'trojan', 'shadowsocks'];
    const skipTags = ['direct', 'block', 'dns-out', 'api'];

    // Find the proxy outbound — by tag or by protocol
    const proxyOutbound = json.outbounds?.find(
      (o) => o.tag === 'proxy' || (proxyProtocols.includes(o.protocol || '') && !skipTags.includes(o.tag || ''))
    );
    if (!proxyOutbound) return null;

    const protocol = (proxyOutbound.protocol || 'vless') as ServerConfig['protocol'];
    const stream = proxyOutbound.streamSettings;
    const country = detectCountry(name);

    // vnext-based protocols (vless, vmess)
    const vnext = proxyOutbound.settings?.vnext?.[0];
    if (vnext) {
      return {
        id: crypto.randomUUID(),
        name,
        protocol,
        address: vnext.address || '',
        port: vnext.port || 443,
        uuid: vnext.users?.[0]?.id,
        transport: stream?.network || 'tcp',
        security: stream?.security || 'none',
        fingerprint: stream?.realitySettings?.fingerprint || stream?.tlsSettings?.fingerprint,
        publicKey: stream?.realitySettings?.publicKey,
        sni: stream?.realitySettings?.serverName || stream?.tlsSettings?.serverName,
        shortId: stream?.realitySettings?.shortId,
        path: stream?.xhttpSettings?.path || stream?.wsSettings?.path,
        flow: vnext.users?.[0]?.flow || undefined,
        encryption: vnext.users?.[0]?.encryption || 'none',
        country: country?.name,
        countryCode: country?.code,
        rawLink: JSON.stringify(json),
        rawConfig: json,
      };
    }

    // servers-based protocols (trojan, shadowsocks)
    const server = proxyOutbound.settings?.servers?.[0];
    if (server) {
      return {
        id: crypto.randomUUID(),
        name,
        protocol: protocol === 'shadowsocks' ? 'shadowsocks' : 'trojan',
        address: server.address || '',
        port: server.port || 443,
        password: server.password,
        encryption: server.method,
        transport: stream?.network || 'tcp',
        security: stream?.security || 'none',
        sni: stream?.realitySettings?.serverName || stream?.tlsSettings?.serverName,
        country: country?.name,
        countryCode: country?.code,
        rawLink: JSON.stringify(json),
        rawConfig: json,
      };
    }

    return null;
  } catch {
    return null;
  }
}

// ========== Fetch Subscription ==========

export async function fetchSubscription(
  url: string,
  name?: string,
  existingId?: string
): Promise<Subscription> {
  const id = existingId || crypto.randomUUID();

  try {
    let text: string;

    // Use Rust-side fetch to bypass CORS in Tauri WebView
    const isTauri = !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    if (isTauri) {
      const { invoke } = await import('@tauri-apps/api/core');
      text = await invoke('fetch_url', { url });
    } else {
      // Dev mode fallback — use Vite CORS proxy to bypass browser restrictions
      const proxyUrl = `/api/proxy?url=${encodeURIComponent(url)}`;
      const response = await fetch(proxyUrl);
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      text = await response.text();
    }
    let servers: ServerConfig[] = [];

    // Try JSON array first (DoodleVPN-style full configs)
    try {
      const jsonArr = JSON.parse(text.trim());
      if (Array.isArray(jsonArr)) {
        servers = jsonArr
          .map((cfg: XrayJsonConfig) => parseXrayJsonConfig(cfg))
          .filter((s): s is ServerConfig => s !== null)
          .map((s) => ({ ...s, subscriptionId: id }));
      }
    } catch {
      // Not JSON — try Base64 then plain text
      let decoded: string;
      try {
        decoded = atob(text.trim());
      } catch {
        decoded = text;
      }
      servers = parseMultipleLinks(decoded).map((s) => ({ ...s, subscriptionId: id }));
    }

    return {
      id,
      name: name || new URL(url).hostname,
      url,
      servers,
      updatedAt: new Date().toISOString(),
    };
  } catch (error) {
    throw new Error(
      `Failed to fetch subscription: ${error instanceof Error ? error.message : 'Unknown error'}`
    );
  }
}

export async function refreshSubscription(sub: Subscription): Promise<Subscription> {
  const updated = await fetchSubscription(sub.url, sub.name, sub.id);
  return updated;
}
