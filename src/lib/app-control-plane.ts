import type { ServerConfig, Subscription, SystemProxyMode } from '../stores/app-store';
import { useAppStore } from '../stores/app-store';
import { isClosedControlPlaneEnabled, isNetworkExtensionOnlyBuild } from './build-policy';
import { getActiveRoutingRules, resolveSystemProxyModeForRouting } from './connect-helpers';
import { getServerSelectionKey, isAutoSelectCandidate, rankAutoLocationCandidates } from './server-selection';
import { desktopBridge } from '../platform/tauri/desktop-bridge';

export { findLegacyDoodleSubscriptionUrl, findLegacyDoodleSubscriptionUrls } from './legacy-subscription';

const CLOSED_SUBSCRIPTION_ID = 'doodlevpn-app';
const LOCATION_ID_PREFIX = 'app-location:';
const AUTO_LOCATION_ID = 'auto';
const MAX_LOCATION_CATALOG_ITEMS = 256;
const MAX_ACTIVE_LOCATIONS = 128;
// ponytail: KZ stays available manually and as a last fallback; remove this
// demotion when backend health verifies real web egress instead of only TLS.
const MAC_AUTO_DEPRIORITIZED_LOCATION_IDS = ['kz'] as const;
let localPreviewLoggedIn = false;

export interface AppApiSubscriptionSummary {
  active: boolean;
  device_allowed?: boolean | null;
  remnawave_status?: string;
  expires_at?: string;
  reason?: string | null;
  update_notice?: {
    minimum_version: string;
  } | null;
  user_uuid?: string | null;
  username?: string | null;
  anti_jammer?: {
    limit_bytes: number;
    used_bytes: number;
    remaining_bytes: number;
    low_balance: boolean;
    exhausted: boolean;
    state?: string;
  } | null;
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

export interface AppApiControlPlaneSnapshot {
  session: AppApiSessionStatus;
  locations: AppApiLocation[];
}

export function isClosedLocationServer(server: ServerConfig | null | undefined): boolean {
  return !!server?.id?.startsWith(LOCATION_ID_PREFIX);
}

export function isClosedAutoLocationServer(server: ServerConfig | null | undefined): boolean {
  return server?.id === `${LOCATION_ID_PREFIX}${AUTO_LOCATION_ID}`;
}

export function closedLocationIdFromServer(server: Pick<ServerConfig, 'id'>): string {
  return server.id.startsWith(LOCATION_ID_PREFIX)
    ? server.id.slice(LOCATION_ID_PREFIX.length)
    : server.id;
}

const ANTI_JAMMER_NAME_RE = /обход|блокиров|white|whitelist|bypass|резерв|reserve/i;

/** True when `server` is the "Обход БС" or "Резерв" special location, in either the closed control-plane model (id-based) or legacy subscriptions (name-based). */
export function isAntiJammerOrReserveServer(server: ServerConfig | null | undefined): boolean {
  if (!server) return false;
  if (isClosedLocationServer(server)) {
    const id = closedLocationIdFromServer(server);
    return id === 'bypass' || id === 'reserve';
  }
  return ANTI_JAMMER_NAME_RE.test(server.name);
}

function parseExpireSeconds(value?: string | null): number | undefined {
  if (!value) return undefined;
  const ms = Date.parse(value);
  if (!Number.isFinite(ms) || ms <= 0) return undefined;
  return Math.floor(ms / 1000);
}

function updateNoticeMinimumVersion(session: AppApiSessionStatus | null): string | null {
  const version = session?.subscription?.update_notice?.minimum_version?.trim() ?? '';
  return /^\d+(?:\.\d+){1,3}$/.test(version) ? version : null;
}

function locationToServer(location: AppApiLocation): ServerConfig {
  const id = String(location.id || location.country_code || '').slice(0, 64).toLowerCase();
  const countryCode = String(location.country_code || id).slice(0, 8).toUpperCase();
  const language = useAppStore.getState().language;
  const localizedCountry = language === 'ru' && countryCode === 'US'
    ? 'США'
    : /^[A-Z]{2}$/.test(countryCode)
      ? new Intl.DisplayNames([language], { type: 'region' }).of(countryCode)
      : undefined;
  // The auto-location row's user-facing text is resolved reactively via
  // t('v6AutoLocationName') in ServerRow so a live language change doesn't
  // leave a stale baked-in name until the next location fetch. Only the
  // leading emoji from this stored name is still used, for the row icon.
  const autoName = id === AUTO_LOCATION_ID ? '⚡ Auto' : undefined;
  const name = String(autoName || localizedCountry || location.title || countryCode || id).slice(0, 128);
  return {
    id: `${LOCATION_ID_PREFIX}${id}`,
    name,
    protocol: 'vless',
    address: 'app-control-plane',
    port: 443,
    transport: 'reality',
    security: 'reality',
    country: name,
    countryCode,
    subscriptionId: CLOSED_SUBSCRIPTION_ID,
    rawLink: '',
  };
}

function subscriptionFromSession(session: AppApiSessionStatus | null): Subscription {
  const summary = session?.subscription ?? null;
  const antiJammer = summary?.anti_jammer;
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
    antiJammer: antiJammer
      ? {
          limitBytes: antiJammer.limit_bytes,
          usedBytes: antiJammer.used_bytes,
          remainingBytes: antiJammer.remaining_bytes,
          lowBalance: antiJammer.low_balance,
          exhausted: antiJammer.exhausted,
          state: antiJammer.state,
        }
      : undefined,
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
          anti_jammer: {
            limit_bytes: 30 * 1024 ** 3,
            used_bytes: 21.4 * 1024 ** 3,
            remaining_bytes: 8.6 * 1024 ** 3,
            low_balance: false,
            exhausted: false,
            state: 'active',
          },
        }
      : null,
  };
}

function localPreviewLocations(): AppApiLocationsResponse {
  return {
    locations: [
      { id: 'bypass', country_code: 'AUTO', title: '📡 Обход БС', available: true, sort: 1, healthy_nodes_count: 2 },
      { id: 'reserve', country_code: 'AUTO', title: '🔁 Резерв', available: true, sort: 2, healthy_nodes_count: 1 },
      { id: 'nl', country_code: 'NL', title: 'Нидерланды', available: true, sort: 10, healthy_nodes_count: 1 },
      { id: 'de', country_code: 'DE', title: 'Германия', available: true, sort: 20, healthy_nodes_count: 1 },
      { id: 'us', country_code: 'US', title: 'США', available: true, sort: 30, healthy_nodes_count: 1 },
    ],
  };
}

export async function appApiSessionStatus(): Promise<AppApiSessionStatus> {
  if (!isTauriRuntime()) return localPreviewSession();
  return await desktopBridge.appApiSessionStatus();
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
  return await desktopBridge.appApiExchangeCode(normalizedCode);
}

export async function appApiExchangeLegacySubscription(subscriptionUrl: string): Promise<AppApiSessionStatus> {
  if (!isTauriRuntime()) {
    localPreviewLoggedIn = true;
    return localPreviewSession(true);
  }
  return await desktopBridge.appApiExchangeLegacySubscription(subscriptionUrl);
}

export async function appApiRefresh(): Promise<AppApiSessionStatus> {
  if (!isTauriRuntime()) return localPreviewSession();
  return await desktopBridge.appApiRefresh();
}

export async function appApiLogout(): Promise<void> {
  if (!isTauriRuntime()) {
    localPreviewLoggedIn = false;
    useAppStore.setState({
      status: 'disconnected',
      activeServer: null,
      lastSelectedServerKey: null,
      servers: [],
      subscriptions: [],
      connectedAt: null,
      currentDownload: 0,
      currentUpload: 0,
      speedHistory: [],
      totalDown: 0,
      totalUp: 0,
      appSessionLoggedIn: false,
      appSessionDeviceAllowed: null,
    });
    return;
  }
  try {
    await desktopBridge.appApiLogout();
  } finally {
    useAppStore.setState({
      status: 'disconnected',
      activeServer: null,
      lastSelectedServerKey: null,
      servers: [],
      subscriptions: [],
      connectedAt: null,
      currentDownload: 0,
      currentUpload: 0,
      speedHistory: [],
      totalDown: 0,
      totalUp: 0,
      appSessionLoggedIn: false,
      appSessionDeviceAllowed: null,
    });
  }
}

export async function appApiLocations(): Promise<AppApiLocationsResponse> {
  if (!isTauriRuntime()) return localPreviewLocations();
  return await desktopBridge.appApiLocations();
}

export async function appApiSubscriptionStatus(): Promise<AppApiSubscriptionSummary> {
  if (!isTauriRuntime()) return localPreviewSession(true).subscription!;
  return await desktopBridge.appApiSubscriptionStatus();
}

export async function appApiControlPlaneSnapshot(
  sessionOverride?: AppApiSessionStatus | null,
): Promise<AppApiControlPlaneSnapshot> {
  const baseSession = sessionOverride ?? await appApiSessionStatus();
  if (!baseSession.logged_in) return { session: baseSession, locations: [] };

  const [subscription, response] = await Promise.all([
    appApiSubscriptionStatus(),
    appApiLocations(),
  ]);
  return {
    session: { ...baseSession, subscription },
    locations: response.locations,
  };
}

export function syncClosedLocationsToStore(
  session: AppApiSessionStatus | null,
  locations: AppApiLocation[],
) {
  if (!isClosedControlPlaneEnabled()) return;

  const state = useAppStore.getState();
  if (!session?.logged_in) {
    useAppStore.setState({
      servers: [],
      subscriptions: state.subscriptions.filter((subscription) => subscription.url !== 'app://doodlevpn'),
      activeServer: null,
      lastSelectedServerKey: null,
      backendUpdateMinimumVersion: null,
    });
    return;
  }
  const previousPings = new Map(state.servers.map((server) => [server.id, server.ping]));
  const locationServers = locations
    .slice(0, MAX_LOCATION_CATALOG_ITEMS)
    .filter((location) => location.available !== false)
    .slice(0, MAX_ACTIVE_LOCATIONS)
    .sort((a, b) => (a.sort ?? 0) - (b.sort ?? 0) || a.title.localeCompare(b.title))
    .map(locationToServer)
    .map((server) => ({ ...server, ping: previousPings.get(server.id) }));
  const servers = locationServers.length === 0
    ? []
    : [locationToServer({ id: AUTO_LOCATION_ID, country_code: 'AUTO', title: 'Auto', available: true }), ...locationServers];
  const subscription = subscriptionFromSession(session);
  const activeId = state.activeServer?.id;
  const activeServer = servers.find((server) => server.id === activeId) || servers[0] || null;

  useAppStore.setState({
    servers,
    subscriptions: [subscription],
    activeServer,
    lastSelectedServerKey: activeServer ? getServerSelectionKey(activeServer) : null,
    backendUpdateMinimumVersion: updateNoticeMinimumVersion(session),
  });
}

export async function buildAppConnectLocationRequestFromState(
  server: ServerConfig,
  proxyModeOverride?: 'system-proxy' | 'tun',
  systemProxyModeOverride?: SystemProxyMode,
) {
  const state = useAppStore.getState();
  const autoCandidates = isClosedAutoLocationServer(server)
    ? await (async () => {
        let countries = state.servers
          .filter((candidate) => isClosedLocationServer(candidate)
            && !isClosedAutoLocationServer(candidate)
            && /^[A-Z]{2}$/.test(candidate.countryCode || '')
            && isAutoSelectCandidate(candidate));
        const macAppStore = isNetworkExtensionOnlyBuild();
        if (macAppStore) {
          countries = await Promise.all(countries.map(async (candidate) => {
            try {
              const result = await desktopBridge.command<{ ping_ms?: number }>('app_ping_location', {
                locationId: closedLocationIdFromServer(candidate),
                serverId: candidate.id,
              });
              return { ...candidate, ping: Number.isFinite(result.ping_ms) ? result.ping_ms : -1 };
            } catch {
              return { ...candidate, ping: -1 };
            }
          }));
        }
        const rankedCountries = rankAutoLocationCandidates(
          countries,
          macAppStore ? MAC_AUTO_DEPRIORITIZED_LOCATION_IDS : [],
        );
        const reserve = state.servers.find((candidate) => closedLocationIdFromServer(candidate) === 'reserve');
        return reserve ? [...rankedCountries.slice(0, 2), reserve] : rankedCountries.slice(0, 3);
      })()
    : [server];
  const resolvedServer = autoCandidates[0];
  if (!resolvedServer) throw new Error('Нет доступных VPN-локаций для автовыбора');
  const proxyMode = proxyModeOverride ?? state.proxyMode;
  const routingRules = proxyMode === 'tun' ? await getActiveRoutingRules() : [];
  const systemProxyMode = resolveSystemProxyModeForRouting(
    proxyMode,
    systemProxyModeOverride ?? state.systemProxyMode,
    routingRules,
  );

  return {
    location_id: closedLocationIdFromServer(resolvedServer),
    fallback_location_ids: autoCandidates.slice(1).map(closedLocationIdFromServer),
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
  return await desktopBridge.appConnectLocation(request);
}
