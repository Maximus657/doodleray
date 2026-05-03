import type { Subscription, ServerConfig } from '../stores/app-store';
import { parseMultipleLinks, detectCountry } from './parser';

// ========== Xray JSON Config Parser ==========

interface XrayJsonConfig {
  remark?: string;
  remarks?: string;
  outbounds?: XrayOutbound[];
  routing?: {
    rules?: XrayRoutingRule[];
  };
}

interface XrayRoutingRule {
  domain?: string[];
  outboundTag?: string;
  balancerTag?: string;
  [key: string]: unknown;
}

interface XrayOutbound {
  tag?: string;
  protocol?: string;
  settings?: {
    vnext?: Array<{
      address?: string;
      port?: number;
      users?: Array<{
        id?: string;
        encryption?: string;
        security?: string;
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
    tlsSettings?: {
      fingerprint?: string;
      serverName?: string;
      alpn?: string[];
    };
    xhttpSettings?: {
      mode?: string;
      path?: string;
    };
    wsSettings?: {
      path?: string;
      headers?: { Host?: string };
    };
    grpcSettings?: {
      serviceName?: string;
    };
  };
}

function isSupportedXrayOutbound(outbound: XrayOutbound): boolean {
  return outbound.protocol === 'vless' && !!outbound.settings?.vnext?.[0];
}

function getSupportedXrayOutbounds(json: XrayJsonConfig): XrayOutbound[] {
  return json.outbounds?.filter(isSupportedXrayOutbound) || [];
}

function cloneConfigForOutbound(json: XrayJsonConfig, outboundTag?: string): XrayJsonConfig {
  const cloned = JSON.parse(JSON.stringify(json)) as XrayJsonConfig;
  if (!outboundTag) return cloned;

  cloned.routing ||= {};
  cloned.routing.rules ||= [];
  const rules = cloned.routing.rules;

  let routedByBalancer = false;
  for (const rule of rules) {
    if (rule.balancerTag) {
      delete rule.balancerTag;
      rule.outboundTag = outboundTag;
      routedByBalancer = true;
    }
  }

  if (!routedByBalancer) {
    rules.push({ type: 'field', outboundTag });
  }

  return cloned;
}

function parseXrayOutbound(
  json: XrayJsonConfig,
  outbound: XrayOutbound,
  options: { rawConfig?: XrayJsonConfig; name?: string } = {}
): ServerConfig | null {
  try {
    const vnext = outbound.settings?.vnext?.[0];
    if (!vnext) return null;

    const stream = outbound.streamSettings;
    const reality = stream?.realitySettings;
    const tls = stream?.tlsSettings;
    const user = vnext.users?.[0];
    const name =
      options.name ||
      json.remarks ||
      json.remark ||
      outbound.tag ||
      `${vnext.address}:${vnext.port || 443}`;
    const country = detectCountry(name);
    const rawConfig = options.rawConfig || json;

    return {
      id: crypto.randomUUID(),
      name,
      protocol: 'vless',
      address: vnext.address || '',
      port: vnext.port || 443,
      uuid: user?.id,
      transport: stream?.network || 'tcp',
      security: stream?.security || (reality ? 'reality' : tls ? 'tls' : 'none'),
      fingerprint: reality?.fingerprint || tls?.fingerprint,
      publicKey: reality?.publicKey,
      sni: reality?.serverName || tls?.serverName || stream?.wsSettings?.headers?.Host,
      shortId: reality?.shortId,
      host: stream?.wsSettings?.headers?.Host,
      path: stream?.xhttpSettings?.path || stream?.wsSettings?.path || stream?.grpcSettings?.serviceName,
      flow: user?.flow || undefined,
      encryption: user?.encryption || user?.security || 'none',
      alpn: tls?.alpn,
      country: country?.name,
      countryCode: country?.code,
      rawLink: JSON.stringify(rawConfig),
      rawConfig,
    };
  } catch {
    return null;
  }
}

function parseXrayJsonConfig(json: XrayJsonConfig): ServerConfig | null {
  const proxyOutbound =
    json.outbounds?.find((o) => o.tag === 'proxy' && isSupportedXrayOutbound(o)) ||
    getSupportedXrayOutbounds(json)[0];

  if (!proxyOutbound) return null;

  return parseXrayOutbound(json, proxyOutbound, {
    name: json.remarks || json.remark,
    rawConfig: json,
  });
}

function parseXrayJsonSubscription(json: XrayJsonConfig, subscriptionName: string): ServerConfig[] {
  const outbounds = getSupportedXrayOutbounds(json);
  if (outbounds.length === 0) return [];

  return outbounds
    .map((outbound, index) => parseXrayOutbound(json, outbound, {
      name: outbound.tag || json.remarks || json.remark || `${subscriptionName} ${index + 1}`,
      rawConfig: cloneConfigForOutbound(json, outbound.tag),
    }))
    .filter((server): server is ServerConfig => server !== null);
}

// ========== Fetch Subscription ==========

export async function fetchSubscription(
  url: string,
  name?: string,
  existingId?: string
): Promise<Subscription> {
  const id = existingId || crypto.randomUUID();
  const subscriptionName = name || new URL(url).hostname;

  try {
    let text: string;

    // Use Rust-side fetch to bypass CORS in Tauri WebView
    const isTauri =
      typeof window !== 'undefined' &&
      !!(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
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
      const parsedJson = JSON.parse(text.trim());
      if (Array.isArray(parsedJson)) {
        servers = parsedJson
          .map((cfg: XrayJsonConfig) => parseXrayJsonConfig(cfg))
          .filter((s): s is ServerConfig => s !== null)
          .map((s) => ({ ...s, subscriptionId: id }));
      } else if (parsedJson && typeof parsedJson === 'object') {
        servers = parseXrayJsonSubscription(parsedJson as XrayJsonConfig, subscriptionName)
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

    if (servers.length === 0) {
      throw new Error('No supported servers found in subscription');
    }

    return {
      id,
      name: subscriptionName,
      url,
      servers,
      updatedAt: new Date().toISOString(),
    };
  } catch (error) {
    const message =
      error instanceof Error
        ? error.message
        : typeof error === 'string'
          ? error
          : JSON.stringify(error);
    throw new Error(
      `Failed to fetch subscription: ${message || 'Unknown error'}`
    );
  }
}

export async function refreshSubscription(sub: Subscription): Promise<Subscription> {
  const updated = await fetchSubscription(sub.url, sub.name, sub.id);
  return updated;
}
