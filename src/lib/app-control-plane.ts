import { invoke } from '@tauri-apps/api/core';
import type { ServerConfig, Subscription, SystemProxyMode } from '../stores/app-store';
import { useAppStore } from '../stores/app-store';
import { isClosedControlPlaneEnabled } from './build-policy';
import { getActiveRoutingRules, resolveSystemProxyModeForRouting } from './connect-helpers';
import { getServerSelectionKey } from './server-selection';

const CLOSED_SUBSCRIPTION_ID = 'doodlevpn-app';
const LOCATION_ID_PREFIX = 'app-location:';
let localPreviewLoggedIn = false;

export interface AppApiSubscriptionSummary {
  active: boolean;
  device_allowed?: boolean | null;
  remnawave_status?: string;
  expires_at?: string;
  reason?: string | null;
  user_uuid?: string | null;
  username?: string | null;
}

export interface AppApiSessionStatus {
  logged_in: boolean;
  device_id?: string | null;
  access_expires_at?: string | null;
  refresh_expires_at?: string | null;
  subscription?: AppApiSubscriptionSummary | null;
  api_base_url: string;
  closed_control_plane_enabled: boolean;
}

export interface AppApiLocation {
  id: string;
  country_code: string;
  title: string;
  available: boolean;
  sort?: number;
  available_nodes_count?: number;
  healthy_nodes_count?: number | null;
  capacity_label?: string;
}

export interface AppApiLocationsResponse {
  locations: AppApiLocation[];
}

export function isClosedLocationServer(server: ServerConfig | null | undefined): boolean {
  return !!server?.id?.startsWith(LOCATION_ID_PREFIX);
}

export function closedLocationIdFromServer(server: ServerConfig): string {
  return server.id.startsWith(LOCATION_ID_PREFIX)
    ? server.id.slice(LOCATION_ID_PREFIX.length)
    : server.id;
}

function parseExpireSeconds(value?: string | null): number | undefined {
  if (!value) return undefined;
  const ms = Date.parse(value);
  if (!Number.isFinite(ms) || ms <= 0) return undefined;
  return Math.floor(ms / 1000);
}

function locationToServer(location: AppApiLocation): ServerConfig {
  const id = String(location.id || location.country_code || '').toLowerCase();
  const countryCode = (location.country_code || id).toUpperCase();
  return {
    id: `${LOCATION_ID_PREFIX}${id}`,
    name: location.title || countryCode || id,
    protocol: 'vless',
    address: 'app-control-plane',
    port: 443,
    transport: 'reality',
    security: 'reality',
    country: location.title,
    countryCode,
    subscriptionId: CLOSED_SUBSCRIPTION_ID,
    rawLink: '',
  };
}

function subscriptionFromSession(session: AppApiSessionStatus | null): Subscription {
  const summary = session?.subscription ?? null;
  return {
    id: CLOSED_SUBSCRIPTION_ID,
    name: 'DoodleVPN',
    url: 'app://doodlevpn',
    servers: [],
    updatedAt: new Date().toISOString(),
    traffic: {
      upload: 0,
      download: 0,
      expire: parseExpireSeconds(summary?.expires_at),
    },
  };
}

function isTauriRuntime() {
  const tauriInternals = (globalThis as unknown as {
    __TAURI_INTERNALS__?: { invoke?: unknown };
  }).__TAURI_INTERNALS__;
  return typeof tauriInternals?.invoke === 'function';
}

function localPreviewSession(loggedIn = localPreviewLoggedIn): AppApiSessionStatus {
  const now = Date.now();
  return {
    logged_in: loggedIn,
    api_base_url: 'local-preview',
    closed_control_plane_enabled: true,
    access_expires_at: loggedIn ? new Date(now + 60 * 60 * 1000).toISOString() : null,
    refresh_expires_at: loggedIn ? new Date(now + 30 * 24 * 60 * 60 * 1000).toISOString() : null,
    subscription: loggedIn
      ? {
          active: true,
          device_allowed: true,
          remnawave_status: 'ACTIVE',
          expires_at: new Date(now + 262 * 24 * 60 * 60 * 1000).toISOString(),
          username: 'local-preview',
        }
      : null,
  };
}

function localPreviewLocations(): AppApiLocationsResponse {
  return {
    locations: [
      { id: 'auto', country_code: 'AUTO', title: 'Авто-Выбор', available: true, sort: 0, healthy_nodes_count: 5 },
      { id: 'bypass', country_code: 'AUTO', title: 'Обход БС', available: true, sort: 1, healthy_nodes_count: 2 },
      { id: 'nl', country_code: 'NL', title: 'Нидерланды', available: true, sort: 2, healthy_nodes_count: 1 },
      { id: 'de', country_code: 'DE', title: 'Германия', available: true, sort: 3, healthy_nodes_count: 1 },
      { id: 'us', country_code: 'US', title: 'США', available: true, sort: 4, healthy_nodes_count: 1 },
    ],
  };
}

export async function appApiSessionStatus(): Promise<AppApiSessionStatus> {
  if (!isTauriRuntime()) return localPreviewSession();
  return await invoke<AppApiSessionStatus>('app_api_session_status');
}

export async function appApiExchangeCode(code: string): Promise<AppApiSessionStatus> {
  const normalizedCode = code.replace(/\D/g, '').slice(0, 8);
  if (normalizedCode.length !== 8) {
    throw new Error('Введите 8 цифр кода');
  }
  if (!isTauriRuntime()) {
    localPreviewLoggedIn = true;
    return localPreviewSession(true);
  }
  return await invoke<AppApiSessionStatus>('app_api_exchange_code', { request: { code: normalizedCode } });
}

export async function appApiRefresh(): Promise<AppApiSessionStatus> {
  if (!isTauriRuntime()) return localPreviewSession();
  return await invoke<AppApiSessionStatus>('app_api_refresh');
}

export async function appApiLogout(): Promise<void> {
  if (!isTauriRuntime()) {
    localPreviewLoggedIn = false;
    useAppStore.setState({
      activeServer: null,
      lastSelectedServerKey: null,
      servers: [],
      subscriptions: [],
    });
    return;
  }
  await invoke('app_api_logout');
  useAppStore.setState({
    activeServer: null,
    lastSelectedServerKey: null,
    servers: [],
    subscriptions: [],
  });
}

export async function appApiLocations(): Promise<AppApiLocationsResponse> {
  if (!isTauriRuntime()) return localPreviewLocations();
  return await invoke<AppApiLocationsResponse>('app_api_locations');
}

export function syncClosedLocationsToStore(
  session: AppApiSessionStatus | null,
  locations: AppApiLocation[],
) {
  if (!isClosedControlPlaneEnabled()) return;

  const servers = locations
    .filter((location) => location.available !== false)
    .sort((a, b) => (a.sort ?? 0) - (b.sort ?? 0) || a.title.localeCompare(b.title))
    .map(locationToServer);
  const subscription = subscriptionFromSession(session);
  const state = useAppStore.getState();
  const activeId = state.activeServer?.id;
  const activeServer = servers.find((server) => server.id === activeId) || servers[0] || null;

  useAppStore.setState({
    servers,
    subscriptions: [subscription],
    activeServer,
    lastSelectedServerKey: activeServer ? getServerSelectionKey(activeServer) : null,
  });
}

export async function buildAppConnectLocationRequestFromState(
  server: ServerConfig,
  proxyModeOverride?: 'system-proxy' | 'tun',
  systemProxyModeOverride?: SystemProxyMode,
) {
  const state = useAppStore.getState();
  const proxyMode = proxyModeOverride ?? state.proxyMode;
  const routingRules = proxyMode === 'tun' ? await getActiveRoutingRules() : [];
  const systemProxyMode = resolveSystemProxyModeForRouting(
    proxyMode,
    systemProxyModeOverride ?? state.systemProxyMode,
    routingRules,
  );

  return {
    location_id: closedLocationIdFromServer(server),
    proxy_mode: proxyMode,
    system_proxy_mode: systemProxyMode,
    socks_port: state.socksPort,
    http_port: state.httpPort,
    network_stack: state.networkStack,
    dns_mode: state.dnsMode,
    strict_route: state.strictRoute,
    kill_switch: state.killSwitch,
    routing_rules: routingRules,
  };
}

export async function appConnectLocation(server: ServerConfig) {
  const request = await buildAppConnectLocationRequestFromState(server);
  return await invoke('app_connect_location', { request });
}
