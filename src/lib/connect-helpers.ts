/**
 * Shared helper to build the vpn_connect request object.
 * Previously this ~45-line object was copy-pasted in 4 places.
 */
import type { ServerConfig, ProxyMode } from '../stores/app-store';
import { useAppStore } from '../stores/app-store';

export interface ConnectOpts {
  proxyMode: ProxyMode;
  socksPort: number;
  httpPort: number;
  networkStack: string;
  dnsMode: string;
  strictRoute: boolean;
  killSwitch: boolean;
  routingRules: Array<{ rule_type: string; value: string; action: string }>;
}

/** Build the request payload for the `vpn_connect` Tauri command. */
export function buildConnectRequest(server: ServerConfig, opts: ConnectOpts) {
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
    proxy_mode: 'tun',
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

/** Get active routing rules from WorkshopStore (async to avoid circular deps). */
export async function getActiveRoutingRules() {
  const { useWorkshopStore } = await import('../stores/workshop-store');
  return useWorkshopStore.getState().myRules
    .filter(r => r.enabled)
    .map(r => ({ rule_type: r.type, value: r.value, action: r.action }));
}

/**
 * Convenience: build the full connect request pulling current state from stores.
 */
export async function buildConnectRequestFromState(server: ServerConfig) {
  const state = useAppStore.getState();
  const routingRules = await getActiveRoutingRules();
  return buildConnectRequest(server, {
    proxyMode: 'vpn',
    socksPort: state.socksPort,
    httpPort: state.httpPort,
    networkStack: state.networkStack,
    dnsMode: state.dnsMode,
    strictRoute: state.strictRoute,
    killSwitch: state.killSwitch,
    routingRules,
  });
}
