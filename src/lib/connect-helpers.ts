/**
 * Shared helper to build the vpn_connect request object.
 * Previously this ~45-line object was copy-pasted in 4 places.
 */
import type { ServerConfig, ProxyMode, SystemProxyMode } from '../stores/app-store';
import { useAppStore } from '../stores/app-store';

export type RoutingRulePayload = { rule_type: string; value: string; action: string };

export interface ConnectOpts {
  proxyMode: ProxyMode;
  socksPort: number;
  httpPort: number;
  networkStack: string;
  dnsMode: string;
  strictRoute: boolean;
  killSwitch: boolean;
  routingRules: RoutingRulePayload[];
  systemProxyMode?: SystemProxyMode;
}

export function hasDirectAppRoutingRule(routingRules: RoutingRulePayload[]): boolean {
  return routingRules.some(rule =>
    rule.rule_type === 'exe' &&
    rule.action === 'direct' &&
    rule.value.trim().length > 0
  );
}

export function resolveSystemProxyModeForRouting(
  proxyMode: ProxyMode,
  requestedSystemProxyMode: SystemProxyMode | undefined,
  routingRules: RoutingRulePayload[],
): SystemProxyMode {
  const normalizedSystemProxyMode =
    !requestedSystemProxyMode || requestedSystemProxyMode === 'clear'
      ? 'unchanged'
      : requestedSystemProxyMode;

  if (
    proxyMode === 'tun' &&
    normalizedSystemProxyMode === 'set' &&
    hasDirectAppRoutingRule(routingRules)
  ) {
    return 'unchanged';
  }

  return normalizedSystemProxyMode;
}

/** Build the request payload for the `vpn_connect` Tauri command. */
export function buildConnectRequest(server: ServerConfig, opts: ConnectOpts) {
  const requestedSystemProxyMode = opts.systemProxyMode ?? useAppStore.getState().systemProxyMode;
  const systemProxyMode = resolveSystemProxyModeForRouting(
    opts.proxyMode,
    requestedSystemProxyMode,
    opts.routingRules,
  );

  return {
    server_address: server.address,
    server_port: server.port,
    protocol: server.protocol,
    uuid: server.uuid || null,
    password: server.password || null,
    transport: server.transport,
    security: server.security,
    sni: server.sni || null,
    host: server.host || null,
    path: server.path || null,
    fingerprint: server.fingerprint || null,
    public_key: server.publicKey || null,
    short_id: server.shortId || null,
    flow: server.flow || null,
    proxy_mode: opts.proxyMode,
    system_proxy_mode: systemProxyMode,
    socks_port: opts.socksPort,
    http_port: opts.httpPort,
    network_stack: opts.networkStack,
    dns_mode: opts.dnsMode,
    strict_route: opts.strictRoute,
    routing_rules: opts.routingRules,
    kill_switch: opts.killSwitch,
    // Hysteria2
    obfs_type: server.obfsType || null,
    obfs_password: server.obfsPassword || null,
    up_mbps: server.upMbps || null,
    down_mbps: server.downMbps || null,
    // TUIC
    congestion_control: server.congestionControl || null,
    udp_relay_mode: server.udpRelayMode || null,
    alpn: server.alpn || null,
    // WireGuard
    private_key: server.privateKey || null,
    peer_public_key: server.peerPublicKey || null,
    pre_shared_key: server.preSharedKey || null,
    local_address: server.localAddress || null,
    reserved: server.reserved || null,
    mtu: server.mtu || null,
    workers: server.workers || null,
    // Shadowsocks
    encryption: server.encryption || null,
    // Full raw xray config (DoodleVPN subscriptions)
    raw_xray_config: server.rawConfig || null,
  };
}

/** Build an isolated, non-system-changing request for per-profile HTTP ping probes. */
export function buildPingProbeRequest(server: ServerConfig) {
  return buildConnectRequest(server, {
    proxyMode: 'system-proxy',
    systemProxyMode: 'unchanged',
    socksPort: 10808,
    httpPort: 10809,
    networkStack: 'system',
    dnsMode: 'realip',
    strictRoute: false,
    killSwitch: false,
    routingRules: [],
  });
}

/** Get active routing rules from WorkshopStore (async to avoid circular deps). */
export async function getActiveRoutingRules() {
  const { useWorkshopStore } = await import('../stores/workshop-store');
  return useWorkshopStore.getState().getAllActiveRules()
    .map(r => ({ rule_type: r.type, value: r.value, action: r.action }));
}

/**
 * Convenience: build the full connect request pulling current state from stores.
 * Pass `proxyModeOverride` when switching modes mid-connection.
 */
export async function buildConnectRequestFromState(
  server: ServerConfig,
  proxyModeOverride?: ProxyMode,
  systemProxyModeOverride?: SystemProxyMode,
) {
  const state = useAppStore.getState();
  const proxyMode = proxyModeOverride ?? state.proxyMode;
  const routingRules = proxyMode === 'tun' ? await getActiveRoutingRules() : [];
  return buildConnectRequest(server, {
    proxyMode,
    socksPort: state.socksPort,
    httpPort: state.httpPort,
    networkStack: state.networkStack,
    dnsMode: state.dnsMode,
    strictRoute: state.strictRoute,
    killSwitch: state.killSwitch,
    routingRules,
    systemProxyMode: systemProxyModeOverride,
  });
}
