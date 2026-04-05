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
        }>;
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
    const proxyOutbound = json.outbounds?.find(
      (o) => o.tag === 'proxy' || (o.protocol === 'vless' && o.tag !== 'direct' && o.tag !== 'block' && o.tag !== 'dns-out')
    );

    if (!proxyOutbound || !proxyOutbound.settings?.vnext?.[0]) {
      // Try multi-outbound (like the Антивайтлист config)
      const firstVless = json.outbounds?.find((o) => o.protocol === 'vless');
      if (!firstVless || !firstVless.settings?.vnext?.[0]) return null;

      const vnext = firstVless.settings.vnext[0];
      const stream = firstVless.streamSettings;
      const country = detectCountry(name);

      return {
        id: crypto.randomUUID(),
        name,
        protocol: 'vless',
        address: vnext.address || '',
        port: vnext.port || 443,
        uuid: vnext.users?.[0]?.id,
        transport: stream?.network || 'tcp',
        security: stream?.security || 'none',
        fingerprint: stream?.realitySettings?.fingerprint,
        publicKey: stream?.realitySettings?.publicKey,
        sni: stream?.realitySettings?.serverName,
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

    const vnext = proxyOutbound.settings.vnext[0];
    const stream = proxyOutbound.streamSettings;
    const country = detectCountry(name);

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'vless',
      address: vnext.address || '',
      port: vnext.port || 443,
      uuid: vnext.users?.[0]?.id,
      transport: stream?.network || 'tcp',
      security: stream?.security || 'none',
      fingerprint: stream?.realitySettings?.fingerprint,
      publicKey: stream?.realitySettings?.publicKey,
      sni: stream?.realitySettings?.serverName,
      shortId: stream?.realitySettings?.shortId,
      path: stream?.xhttpSettings?.path || stream?.wsSettings?.path,
      flow: vnext.users?.[0]?.flow || undefined,
      encryption: vnext.users?.[0]?.encryption || 'none',
      country: country?.name,
      countryCode: country?.code,
      rawLink: JSON.stringify(json),
      rawConfig: json,
    };
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
      // Dev mode fallback — browser fetch
      const response = await fetch(url);
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
        const binary = atob(text.trim());
        const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
        decoded = new TextDecoder().decode(bytes);
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
