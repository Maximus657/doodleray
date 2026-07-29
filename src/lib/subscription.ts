import type { Subscription, ServerConfig } from '../stores/app-store';
import { parseMultipleLinks, detectCountry } from './parser';
import { getServerIdentityKey, stableServerId } from './server-selection';
import { desktopBridge } from '../platform/tauri/desktop-bridge';

/// Attach the subscription id, dedupe deterministically by normalized
/// identity (payload order preserved, first occurrence wins), and replace
/// random parse-time ids with stable ids so refreshes never reshuffle
/// selection or ping state.
function finalizeSubscriptionServers(subscriptionId: string, servers: ServerConfig[]): ServerConfig[] {
  const seen = new Set<string>();
  const result: ServerConfig[] = [];
  for (const server of servers) {
    const scoped = { ...server, subscriptionId };
    const identity = getServerIdentityKey(scoped);
    if (seen.has(identity)) continue;
    seen.add(identity);
    result.push({ ...scoped, id: stableServerId(subscriptionId, scoped) });
  }
  return result;
}

interface FetchedSubscriptionPayload {
  text: string;
  userInfo?: string;
  profileTitle?: string;
  contentDisposition?: string;
}

// ========== Xray JSON Config Parser ==========

interface XrayJsonConfig {
  remark?: string;
  remarks?: string;
  outbounds?: XrayOutbound[];
  routing?: {
    balancers?: XrayBalancer[];
    rules?: XrayRoutingRule[];
  };
}

interface XrayBalancer {
  tag?: string;
  selector?: string[];
  [key: string]: unknown;
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
      host?: string;
    };
    wsSettings?: {
      path?: string;
      host?: string;
      headers?: { Host?: string };
    };
    grpcSettings?: {
      serviceName?: string;
    };
  };
}

interface XrayParseContext {
  supportedOutbounds: XrayOutbound[];
  balancers: XrayBalancer[];
}

function isSupportedXrayOutbound(outbound: XrayOutbound): boolean {
  return outbound.protocol === 'vless' && !!outbound.settings?.vnext?.[0];
}

function getSupportedXrayOutbounds(json: XrayJsonConfig): XrayOutbound[] {
  return json.outbounds?.filter(isSupportedXrayOutbound) || [];
}

function getXrayBalancers(json: XrayJsonConfig): XrayBalancer[] {
  return json.routing?.balancers?.filter((balancer) => !!balancer.tag) || [];
}

function createXrayParseContext(json: XrayJsonConfig): XrayParseContext {
  return {
    supportedOutbounds: getSupportedXrayOutbounds(json),
    balancers: getXrayBalancers(json),
  };
}

function outboundMatchesBalancer(outbound: XrayOutbound, balancer: XrayBalancer): boolean {
  const tag = outbound.tag;
  if (!tag) return false;
  const selectors = balancer.selector || [];
  if (selectors.length === 0) return true;

  return selectors.some((selector) =>
    tag === selector ||
    tag.startsWith(selector) ||
    tag.toLowerCase().includes(selector.toLowerCase())
  );
}

function getBalancerOutbounds(context: XrayParseContext, balancer: XrayBalancer): XrayOutbound[] {
  const selected = context.supportedOutbounds.filter((outbound) => outboundMatchesBalancer(outbound, balancer));
  return selected.length > 0 ? selected : context.supportedOutbounds;
}

function isAggregateAutoBalancer(balancer: XrayBalancer, balancerCount: number): boolean {
  const tag = balancer.tag?.toLowerCase() || '';
  if (/entry-pool|urltest|leastping/.test(tag)) return true;
  return balancerCount === 1 && /авто|auto|fast|самый/.test(tag);
}

function getAutoBalancerName(balancer: XrayBalancer): string {
  const tag = balancer.tag || '';
  if (/entry-pool/i.test(tag)) return '⚡ Самый быстрый auto';
  return tag || '⚡ Самый быстрый auto';
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

function cloneConfigForBalancer(json: XrayJsonConfig, balancerTag?: string): XrayJsonConfig {
  const cloned = JSON.parse(JSON.stringify(json)) as XrayJsonConfig;
  if (!balancerTag) return cloned;

  cloned.routing ||= {};
  cloned.routing.rules ||= [];
  const rules = cloned.routing.rules;

  let routedByBalancer = false;
  for (const rule of rules) {
    if (rule.balancerTag) {
      rule.balancerTag = balancerTag;
      routedByBalancer = true;
    }
  }

  if (!routedByBalancer) {
    rules.push({ type: 'field', balancerTag });
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
    const wsHost = stream?.wsSettings?.host || stream?.wsSettings?.headers?.Host;
    const xhttpHost = stream?.xhttpSettings?.host;
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
      sni: reality?.serverName || tls?.serverName || wsHost || xhttpHost,
      shortId: reality?.shortId,
      host: wsHost || xhttpHost,
      path: stream?.xhttpSettings?.path || stream?.wsSettings?.path || stream?.grpcSettings?.serviceName,
      flow: user?.flow || undefined,
      encryption: user?.encryption || user?.security || 'none',
      alpn: tls?.alpn,
      country: country?.name,
      countryCode: country?.code,
      rawLink: '',
      rawConfig,
    };
  } catch {
    return null;
  }
}

function parseXrayJsonConfig(json: XrayJsonConfig): ServerConfig | null {
  const supportedOutbounds = getSupportedXrayOutbounds(json);
  const proxyOutbound =
    supportedOutbounds.find((o) => o.tag === 'proxy') ||
    supportedOutbounds[0];

  if (!proxyOutbound) return null;

  return parseXrayOutbound(json, proxyOutbound, {
    name: json.remarks || json.remark,
    rawConfig: json,
  });
}

function parseXrayJsonSubscription(json: XrayJsonConfig, subscriptionName: string): ServerConfig[] {
  const context = createXrayParseContext(json);
  const balancers = context.balancers;
  const autoBalancer = balancers.find((balancer) => isAggregateAutoBalancer(balancer, balancers.length));
  if (autoBalancer) {
    const autoOutbound = getBalancerOutbounds(context, autoBalancer)[0];
    const autoServer = autoOutbound
      ? parseXrayOutbound(json, autoOutbound, {
        name: getAutoBalancerName(autoBalancer),
        rawConfig: cloneConfigForBalancer(json, autoBalancer.tag),
      })
      : null;

    const outboundServers = context.supportedOutbounds
      .map((outbound, index) => parseXrayOutbound(json, outbound, {
        name: outbound.tag || json.remarks || json.remark || `${subscriptionName} ${index + 1}`,
        rawConfig: cloneConfigForOutbound(json, outbound.tag),
      }))
      .filter((server): server is ServerConfig => server !== null);

    return [autoServer, ...outboundServers].filter((server): server is ServerConfig => server !== null);
  }

  if (balancers.length > 0) {
    const servers = balancers
      .map((balancer, index) => {
        const outbound = getBalancerOutbounds(context, balancer)[0];
        if (!outbound) return null;

        return parseXrayOutbound(json, outbound, {
          name: balancer.tag || `${subscriptionName} ${index + 1}`,
          rawConfig: cloneConfigForBalancer(json, balancer.tag),
        });
      })
      .filter((server): server is ServerConfig => server !== null);

    if (servers.length > 0) return servers;
  }

  const outbounds = context.supportedOutbounds;
  if (outbounds.length === 0) return [];

  return outbounds
    .map((outbound, index) => parseXrayOutbound(json, outbound, {
      name: outbound.tag || json.remarks || json.remark || `${subscriptionName} ${index + 1}`,
      rawConfig: cloneConfigForOutbound(json, outbound.tag),
    }))
    .filter((server): server is ServerConfig => server !== null);
}

function parseSubscriptionUserInfo(value?: string | null): Subscription['traffic'] | undefined {
  if (!value) return undefined;

  const parts = value.split(';').map((part) => part.trim()).filter(Boolean);
  const info: Record<string, number> = {};

  for (const part of parts) {
    const [key, rawValue] = part.split('=').map((item) => item.trim());
    if (!key || !rawValue) continue;

    const parsed = Number(rawValue);
    if (Number.isFinite(parsed)) info[key.toLowerCase()] = parsed;
  }

  if (info.upload === undefined && info.download === undefined && info.total === undefined && info.expire === undefined) {
    return undefined;
  }

  return {
    upload: info.upload || 0,
    download: info.download || 0,
    total: info.total,
    expire: info.expire,
  };
}

function decodeUtf8Base64(value: string): string | undefined {
  try {
    const binary = atob(value);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder().decode(bytes).trim();
  } catch {
    return undefined;
  }
}

function normalizeSubscriptionTitle(value?: string | null): string | undefined {
  if (!value) return undefined;
  const trimmed = value.trim();
  if (!trimmed) return undefined;

  const base64Match = trimmed.match(/^base64:(.+)$/i);
  if (base64Match) return decodeUtf8Base64(base64Match[1]);

  try {
    const decoded = decodeURIComponent(trimmed);
    return decoded.trim() || undefined;
  } catch {
    return trimmed;
  }
}

function parseContentDispositionName(value?: string | null): string | undefined {
  if (!value) return undefined;

  const filenameStar = value.match(/filename\*=UTF-8''([^;]+)/i);
  if (filenameStar) return normalizeSubscriptionTitle(filenameStar[1]);

  const filename = value.match(/filename="?([^";]+)"?/i);
  if (!filename) return undefined;

  const cleanName = filename[1].replace(/\.(?:txt|json|yaml|yml|conf)$/i, '');
  return normalizeSubscriptionTitle(cleanName);
}

function getFallbackSubscriptionName(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

async function fetchSubscriptionText(url: string): Promise<FetchedSubscriptionPayload> {
  const isTauri =
    typeof window !== 'undefined' &&
    typeof (window as unknown as {
      __TAURI_INTERNALS__?: { invoke?: unknown };
    }).__TAURI_INTERNALS__?.invoke === 'function';

  if (isTauri) {
    try {
      const result = await desktopBridge.command<{
        body: string;
        subscription_userinfo?: string | null;
        profile_title?: string | null;
        content_disposition?: string | null;
      }>('fetch_subscription_url', { url });
      return {
        text: result.body,
        userInfo: result.subscription_userinfo || undefined,
        profileTitle: result.profile_title || undefined,
        contentDisposition: result.content_disposition || undefined,
      };
    } catch {
      const text = await desktopBridge.command<string>('fetch_url', { url });
      return { text };
    }
  }

  const browserFetch = async (targetUrl: string) => {
    const response = await fetch(targetUrl);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    return {
      text: await response.text(),
      userInfo:
        response.headers.get('subscription-userinfo') ||
        response.headers.get('x-subscription-userinfo') ||
        undefined,
      profileTitle:
        response.headers.get('profile-title') ||
        response.headers.get('x-profile-title') ||
        undefined,
      contentDisposition: response.headers.get('content-disposition') || undefined,
    };
  };

  const isLocalDev =
    typeof window !== 'undefined' &&
    ['localhost', '127.0.0.1', '::1'].includes(window.location.hostname);

  try {
    const result = await browserFetch(url);
    if (result.userInfo || !isLocalDev) return result;
  } catch {
    if (!isLocalDev) throw new Error('Failed to fetch subscription');
  }

  const response = await fetch(`/api/proxy?url=${encodeURIComponent(url)}`);
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
  }

  return {
    text: await response.text(),
    userInfo:
      response.headers.get('subscription-userinfo') ||
      response.headers.get('x-subscription-userinfo') ||
      undefined,
    profileTitle:
      response.headers.get('profile-title') ||
      response.headers.get('x-profile-title') ||
      undefined,
    contentDisposition: response.headers.get('content-disposition') || undefined,
  };
}

// ========== Fetch Subscription ==========

export async function fetchSubscription(
  url: string,
  name?: string,
  existingId?: string
): Promise<Subscription> {
  const id = existingId || crypto.randomUUID();
  const fallbackName = name || getFallbackSubscriptionName(url);

  try {
    const { text, userInfo, profileTitle, contentDisposition } = await fetchSubscriptionText(url);
    const remoteName =
      normalizeSubscriptionTitle(profileTitle) ||
      parseContentDispositionName(contentDisposition);
    const subscriptionName = remoteName || fallbackName;
    let servers: ServerConfig[] = [];

    // Try JSON array first (DoodleVPN-style full configs)
    try {
      const parsedJson = JSON.parse(text.trim());
      if (Array.isArray(parsedJson)) {
        servers = parsedJson
          .map((cfg: XrayJsonConfig) => parseXrayJsonConfig(cfg))
          .filter((s): s is ServerConfig => s !== null);
      } else if (parsedJson && typeof parsedJson === 'object') {
        servers = parseXrayJsonSubscription(parsedJson as XrayJsonConfig, subscriptionName);
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
      servers = parseMultipleLinks(decoded);
    }
    servers = finalizeSubscriptionServers(id, servers);

    if (servers.length === 0) {
      throw new Error('No supported servers found in subscription');
    }

    return {
      id,
      name: subscriptionName,
      url,
      servers,
      updatedAt: new Date().toISOString(),
      traffic: parseSubscriptionUserInfo(userInfo),
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
