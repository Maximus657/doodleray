import { create } from 'zustand';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import {
  buildServerSelectionIndex,
  findMatchingServerInIndex,
  findServerBySelectionKeyInIndex,
  getServerIdentityKey,
  getServerSelectionKey,
} from '../lib/server-selection';
// Trigger HMR

// ========== Types ==========

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'disconnecting';
export type AppUpdatePhase = 'idle' | 'checking' | 'available' | 'downloading' | 'installing' | 'error';

export type ProductMode = 'protected' | 'compatibility' | 'manual';
export type ProxyMode = 'system-proxy' | 'tun';
export type SystemProxyMode = 'set' | 'clear' | 'unchanged';
export type SupportedLanguage = 'ru' | 'en' | 'zh';

export interface ServerConfig {
  id: string;
  name: string;
  protocol: 'vless' | 'vmess' | 'trojan' | 'shadowsocks' | 'hysteria2' | 'tuic' | 'wireguard';
  address: string;
  port: number;
  uuid?: string;
  password?: string;
  transport: string;
  security: string;
  host?: string;
  path?: string;
  sni?: string;
  fingerprint?: string;
  publicKey?: string;
  shortId?: string;
  flow?: string;
  encryption?: string;
  country?: string;
  countryCode?: string;
  subscriptionId?: string;
  ping?: number;
  rawLink: string;
  rawConfig?: unknown;
  // Hysteria2 specific
  obfsType?: string;       // 'salamander'
  obfsPassword?: string;
  upMbps?: number;
  downMbps?: number;
  // TUIC specific
  congestionControl?: string;  // 'bbr' | 'cubic' | 'new_reno'
  udpRelayMode?: string;       // 'native' | 'quic'
  alpn?: string[];
  // WireGuard specific
  privateKey?: string;
  peerPublicKey?: string;
  preSharedKey?: string;
  localAddress?: string[];    // e.g. ['10.0.0.2/32']
  reserved?: number[];        // [0,0,0]
  mtu?: number;
  workers?: number;
}

export interface LogEntry {
  id: string;
  time: string;
  level: 'info' | 'warning' | 'error' | 'success';
  message: string;
}

export interface Subscription {
  id: string;
  name: string;
  url: string;
  servers: ServerConfig[];
  updatedAt: string;
  traffic?: {
    upload: number;
    download: number;
    total?: number;
    expire?: number;
  };
}

export interface SpeedPoint {
  time: string;
  download: number;
  upload: number;
}

export interface ServerPingUpdate {
  id: string;
  ping: number;
}

export interface AppState {
  status: ConnectionStatus;
  activeServer: ServerConfig | null;
  lastSelectedServerKey: string | null;
  productMode: ProductMode;
  proxyMode: ProxyMode;
  systemProxyMode: SystemProxyMode;

  servers: ServerConfig[];
  subscriptions: Subscription[];
  speedHistory: SpeedPoint[];
  currentDownload: number;
  currentUpload: number;
  totalDown: number;
  totalUp: number;
  logs: LogEntry[];
  socksPort: number;
  httpPort: number;
  autoStart: boolean;
  silentAdminAutostart: boolean;
  theme: 'dark' | 'light';
  language: SupportedLanguage;
  networkStack: 'mixed' | 'system' | 'gvisor';
  dnsMode: 'fakeip' | 'realip';
  strictRoute: boolean;
  killSwitch: boolean;
  autoSelectFastest: boolean;
  subAutoUpdateMinutes: number;
  connectedAt: number | null;
  alwaysRunAdmin: boolean;
  autoConnectOnStartup: boolean;
  availableUpdate: string | null;
  updatePhase: AppUpdatePhase;
  updateStatus: string;
  updateProgress: number | null;
  showStats: boolean; // Hide/show statistics on dashboard

  setStatus: (status: ConnectionStatus) => void;
  setActiveServer: (server: ServerConfig | null) => void;
  setProductMode: (mode: ProductMode) => void;
  setProxyMode: (mode: ProxyMode) => void;
  setSystemProxyMode: (mode: SystemProxyMode) => void;

  setNetworkStack: (stack: 'mixed' | 'system' | 'gvisor') => void;
  setDnsMode: (mode: 'fakeip' | 'realip') => void;
  setStrictRoute: (strict: boolean) => void;
  setKillSwitch: (on: boolean) => void;
  setAutoSelectFastest: (on: boolean) => void;
  setSubAutoUpdateMinutes: (mins: number) => void;
  setConnectedAt: (ts: number | null) => void;
  setAlwaysRunAdmin: (on: boolean) => void;
  setAutoConnectOnStartup: (on: boolean) => void;
  setSilentAdminAutostart: (on: boolean) => void;
  setShowStats: (show: boolean) => void;

  addServer: (server: ServerConfig) => void;
  removeServer: (id: string) => void;
  removeAllManualServers: () => void;
  setServers: (servers: ServerConfig[]) => void;
  addSubscription: (sub: Subscription) => void;
  removeSubscription: (id: string) => void;
  updateSubscription: (id: string, newSub: Subscription) => void;
  updateServerPing: (id: string, ping: number) => void;
  updateServerPings: (updates: ServerPingUpdate[]) => void;
  addSpeedPoint: (point: SpeedPoint) => void;
  setCurrentSpeed: (download: number, upload: number) => void;
  setSocksPort: (port: number) => void;
  setHttpPort: (port: number) => void;
  setTheme: (theme: 'dark' | 'light') => void;
  setLanguage: (lang: SupportedLanguage) => void;
  addLog: (level: LogEntry['level'], message: string) => void;
  clearLogs: () => void;
  wipeData: () => void;
  setAvailableUpdate: (version: string | null) => void;
  setUpdateState: (state: Partial<Pick<AppState, 'availableUpdate' | 'updatePhase' | 'updateStatus' | 'updateProgress'>>) => void;
  addTraffic: (dl: number, ul: number) => void;
  resetTraffic: () => void;
}

const isTauriRuntime = () =>
  typeof window !== 'undefined' &&
  typeof (window as unknown as {
    __TAURI_INTERNALS__?: { invoke?: unknown };
  }).__TAURI_INTERNALS__?.invoke === 'function';

const safeLocalGet = (name: string) => {
  try {
    return localStorage.getItem(name);
  } catch {
    return null;
  }
};

const safeLocalSet = (name: string, value: string) => {
  try {
    localStorage.setItem(name, value);
  } catch (err) {
    console.warn('[storage] localStorage write failed', err);
  }
};

const safeLocalRemove = (name: string) => {
  try {
    localStorage.removeItem(name);
  } catch {
    // Ignore cleanup failures; secure storage/fallback remains canonical.
  }
};

export function detectInitialLanguage(): SupportedLanguage {
  if (typeof navigator === 'undefined') return 'en';

  const locales = [
    ...(Array.isArray(navigator.languages) ? navigator.languages : []),
    navigator.language,
  ]
    .filter(Boolean)
    .map((locale) => locale.toLowerCase());

  if (locales.some((locale) => locale.startsWith('zh'))) return 'zh';
  if (locales.some((locale) => /^(ru|uk|be|kk|ky|uz-cyrl|sr-cyrl)(-|$)/.test(locale) || locale === 'ru')) return 'ru';

  return 'en';
}

const secureStorage: StateStorage<Promise<void> | void> = {
  async getItem(name) {
    if (!isTauriRuntime()) return safeLocalGet(name);

    const legacyValue = safeLocalGet(name);
    if (legacyValue !== null) {
      try {
        await invoke('secure_store_set', { key: name, value: legacyValue });
        safeLocalRemove(name);
      } catch (err) {
        console.warn('[storage] secure migration failed, keeping local fallback', err);
      }
      return legacyValue;
    }

    try {
      const value = await invoke<string | null>('secure_store_get', { key: name });
      return value;
    } catch (err) {
      console.warn('[storage] secure read failed, using local fallback', err);
      return null;
    }
  },
  async setItem(name, value) {
    if (!isTauriRuntime()) {
      safeLocalSet(name, value);
      return;
    }

    // Write local first so a Keychain/IPC failure never loses newly added servers.
    safeLocalSet(name, value);
    try {
      await invoke('secure_store_set', { key: name, value });
      safeLocalRemove(name);
    } catch (err) {
      console.warn('[storage] secure write failed, keeping local fallback', err);
    }
  },
  async removeItem(name) {
    if (!isTauriRuntime()) {
      safeLocalRemove(name);
      return;
    }

    safeLocalRemove(name);
    try {
      await invoke('secure_store_delete', { key: name });
    } catch (err) {
      console.warn('[storage] secure delete failed', err);
    }
  },
};

function compactSubscriptionForStore(sub: Subscription): Subscription {
  return {
    ...sub,
    servers: [],
  };
}

function compactServerReference(server: ServerConfig | null): ServerConfig | null {
  if (!server) return null;
  return {
    ...server,
    rawLink: server.rawConfig ? '' : server.rawLink,
    rawConfig: undefined,
  };
}

function resolveStoredActiveServer(
  activeServer: ServerConfig | null,
  lastSelectedServerKey: string | null | undefined,
  servers: ServerConfig[],
): ServerConfig | null {
  const index = buildServerSelectionIndex(servers);
  return findMatchingServerInIndex(activeServer, index) || findServerBySelectionKeyInIndex(lastSelectedServerKey, index);
}

function activeServerUpdate(
  activeServer: ServerConfig | null,
  lastSelectedServerKey: string | null | undefined,
  servers: ServerConfig[],
) {
  const resolved = resolveStoredActiveServer(activeServer, lastSelectedServerKey, servers);
  return {
    activeServer: resolved,
    lastSelectedServerKey: resolved
      ? getServerSelectionKey(resolved)
      : lastSelectedServerKey ?? (activeServer ? getServerSelectionKey(activeServer) : null),
  };
}

function getServerDedupKey(server: ServerConfig): string {
  const scope = server.subscriptionId ? `sub:${server.subscriptionId}` : `manual:${server.id}`;
  return `${scope}\u0000${getServerIdentityKey(server)}`;
}

function dedupeServersByScopedIdentity(servers: ServerConfig[]): ServerConfig[] {
  const seen = new Set<string>();
  return servers.filter((server) => {
    const key = getServerDedupKey(server);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function normalizeSystemProxyMode(mode: SystemProxyMode | undefined, _proxyMode?: ProxyMode): SystemProxyMode {
  if (mode === 'clear' || !mode) return 'unchanged';
  return mode;
}

function transportForProductMode(mode: ProductMode): {
  productMode: ProductMode;
  proxyMode: ProxyMode;
  systemProxyMode: SystemProxyMode;
} {
  switch (mode) {
    case 'compatibility':
      return { productMode: mode, proxyMode: 'system-proxy', systemProxyMode: 'set' };
    case 'manual':
      return { productMode: mode, proxyMode: 'system-proxy', systemProxyMode: 'unchanged' };
    case 'protected':
    default:
      return { productMode: 'protected', proxyMode: 'tun', systemProxyMode: 'set' };
  }
}

function productModeFromTransport(proxyMode: ProxyMode, systemProxyMode: SystemProxyMode): ProductMode {
  if (proxyMode === 'tun') return 'protected';
  return systemProxyMode === 'set' ? 'compatibility' : 'manual';
}

function compactStateForPersist(state: AppState): Partial<AppState> {
  const excluded = new Set([
    'status',
    'speedHistory',
    'currentDownload',
    'currentUpload',
    'totalDown',
    'totalUp',
    'logs',
    'availableUpdate',
    'updatePhase',
    'updateStatus',
    'updateProgress',
  ]);

  return Object.fromEntries(
    Object.entries(state as any)
      .filter(([key]) => !excluded.has(key))
      .map(([key, value]) => {
        if (key === 'subscriptions' && Array.isArray(value)) {
          return [key, value.map(compactSubscriptionForStore)];
        }
        if (key === 'activeServer') {
          return [key, compactServerReference(value as ServerConfig | null)];
        }
        return [key, value];
      })
  ) as Partial<AppState>;
}

function applyServerPingUpdates(state: AppState, updates: ServerPingUpdate[]): Partial<AppState> {
  if (updates.length === 0) return {};

  const pingById = new Map<string, number>();
  for (const update of updates) {
    pingById.set(update.id, update.ping);
  }

  let serversChanged = false;
  const servers = state.servers.map((server) => {
    if (!pingById.has(server.id)) return server;
    const ping = pingById.get(server.id)!;
    if (server.ping === ping) return server;
    serversChanged = true;
    return { ...server, ping };
  });

  let activeServer = state.activeServer;
  if (activeServer && pingById.has(activeServer.id)) {
    const ping = pingById.get(activeServer.id)!;
    if (activeServer.ping !== ping) {
      activeServer = { ...activeServer, ping };
    }
  }

  return {
    servers: serversChanged ? servers : state.servers,
    activeServer,
  };
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      status: 'disconnected',
      activeServer: null,
      lastSelectedServerKey: null,
      productMode: 'protected',
      proxyMode: 'tun',
      systemProxyMode: 'set',

      servers: [],
      subscriptions: [],
      speedHistory: [],
      currentDownload: 0,
      currentUpload: 0,
      totalDown: 0,
      totalUp: 0,
      logs: [],
      socksPort: 10808,
      httpPort: 10809,
      autoStart: true,
      silentAdminAutostart: false,
      theme: 'dark',
      language: detectInitialLanguage(),
      networkStack: 'mixed',
      dnsMode: 'fakeip',
      strictRoute: true,
      killSwitch: false,
      autoSelectFastest: true,
      subAutoUpdateMinutes: 180,
      connectedAt: null,
      alwaysRunAdmin: false,
      autoConnectOnStartup: false,
      availableUpdate: null,
      updatePhase: 'idle',
      updateStatus: '',
      updateProgress: null,
      showStats: false,

      setStatus: (status) => set({ status }),
      setActiveServer: (server) => set({
        activeServer: server,
        lastSelectedServerKey: server ? getServerSelectionKey(server) : null,
      }),
      setProductMode: (mode) => set(transportForProductMode(mode)),
      setProxyMode: (mode) => set((state) => ({
        proxyMode: mode,
        systemProxyMode: normalizeSystemProxyMode(state.systemProxyMode, mode),
        productMode: productModeFromTransport(
          mode,
          normalizeSystemProxyMode(state.systemProxyMode, mode),
        ),
      })),
      setSystemProxyMode: (mode) => set((state) => ({
        systemProxyMode: normalizeSystemProxyMode(mode, state.proxyMode),
        productMode: productModeFromTransport(
          state.proxyMode,
          normalizeSystemProxyMode(mode, state.proxyMode),
        ),
      })),

      setNetworkStack: (stack) => set({ networkStack: stack }),
      setDnsMode: (mode) => set({ dnsMode: mode }),
      setStrictRoute: (strict) => set({ strictRoute: strict }),
      setKillSwitch: (on) => set({ killSwitch: on }),
      setAutoSelectFastest: (on) => set({ autoSelectFastest: on }),
      setSubAutoUpdateMinutes: (mins) => set({ subAutoUpdateMinutes: mins }),
      setConnectedAt: (ts) => set({ connectedAt: ts }),
      setAlwaysRunAdmin: (on) => set({ alwaysRunAdmin: on }),
      setAutoConnectOnStartup: (on) => set({ autoConnectOnStartup: on }),
      setSilentAdminAutostart: (on) => set({ silentAdminAutostart: on }),
      setShowStats: (show) => set({ showStats: show }),



      addServer: (server) => set((s) => ({ servers: [...s.servers, server] })),
      removeServer: (id) => set((s) => {
        const removingActive = s.activeServer?.id === id;
        return {
          servers: s.servers.filter((s2) => s2.id !== id),
          activeServer: removingActive ? null : s.activeServer,
          lastSelectedServerKey: removingActive ? null : s.lastSelectedServerKey,
        };
      }),
      removeAllManualServers: () => set((s) => ({
        servers: s.servers.filter((srv) => srv.subscriptionId !== undefined),
        activeServer: (!s.activeServer?.subscriptionId) ? null : s.activeServer,
        lastSelectedServerKey: (!s.activeServer?.subscriptionId) ? null : s.lastSelectedServerKey,
      })),
      setServers: (servers) => set((s) => ({
        servers,
        ...activeServerUpdate(s.activeServer, s.lastSelectedServerKey, servers),
      })),

      addSubscription: (sub) => set((s) => {
        // If subscription with same URL already exists, replace it
        const existing = s.subscriptions.find((x) => x.url === sub.url);
        const compactSub = compactSubscriptionForStore(sub);
        const newSubscriptions = existing
          ? s.subscriptions.map((x) => x.url === sub.url ? compactSub : x)
          : [...s.subscriptions, compactSub];
        const newServers = existing
          ? [...s.servers.filter((srv) => srv.subscriptionId !== existing.id), ...sub.servers]
          : [...s.servers, ...sub.servers];
        const deduped = dedupeServersByScopedIdentity(newServers);
        return {
          subscriptions: newSubscriptions,
          servers: deduped,
          ...activeServerUpdate(s.activeServer, s.lastSelectedServerKey, deduped),
        };
      }),
      removeSubscription: (id) => set((s) => {
        const removingActive = s.activeServer?.subscriptionId === id;
        return {
          subscriptions: s.subscriptions.filter((sub) => sub.id !== id),
          servers: s.servers.filter((srv) => srv.subscriptionId !== id),
          activeServer: removingActive ? null : s.activeServer,
          lastSelectedServerKey: removingActive ? null : s.lastSelectedServerKey,
        };
      }),
      updateSubscription: (id, newSub) => set((s) => {
        const newServers = [
          ...s.servers.filter((srv) => srv.subscriptionId !== id),
          ...newSub.servers,
        ];
        const deduped = dedupeServersByScopedIdentity(newServers);
        return {
          subscriptions: s.subscriptions.map((sub) => sub.id === id ? compactSubscriptionForStore(newSub) : sub),
          servers: deduped,
          ...activeServerUpdate(s.activeServer, s.lastSelectedServerKey, deduped),
        };
      }),

      updateServerPing: (id, ping) => set((s) => applyServerPingUpdates(s, [{ id, ping }])),
      updateServerPings: (updates) => set((s) => applyServerPingUpdates(s, updates)),

      addSpeedPoint: (point) => set((s) => ({
        speedHistory: [...s.speedHistory.slice(-239), point],
      })),
      setCurrentSpeed: (download, upload) => set({
        currentDownload: download,
        currentUpload: upload,
      }),

      setSocksPort: (port) => set({ socksPort: port }),
      setHttpPort: (port) => set({ httpPort: port }),
      setTheme: (theme) => set({ theme }),
      setLanguage: (lang) => set({ language: lang }),
      addLog: (level, message) => set((s) => ({
        logs: [...s.logs.slice(-99), { id: crypto.randomUUID(), time: new Date().toLocaleTimeString(), level, message }],
      })),
      clearLogs: () => set({ logs: [] }),
      wipeData: () => set({ servers: [], subscriptions: [], activeServer: null, lastSelectedServerKey: null }),
      setAvailableUpdate: (version) => set({ availableUpdate: version }),
      setUpdateState: (state) => set(state),
      addTraffic: (dl, ul) => set((s) => ({ totalDown: s.totalDown + dl, totalUp: s.totalUp + ul })),
      resetTraffic: () => set({ totalDown: 0, totalUp: 0, speedHistory: [], currentDownload: 0, currentUpload: 0 }),
    }),
    {
      name: 'doodleray-storage',
      storage: createJSONStorage(() => secureStorage),
      partialize: compactStateForPersist,
      merge: (persisted, current) => {
        const persistedState = persisted as Partial<AppState> | undefined;
        const merged = {
          ...current,
          ...persistedState,
          language: persistedState?.language ?? current.language,
        } as AppState;
        merged.systemProxyMode = normalizeSystemProxyMode(merged.systemProxyMode, merged.proxyMode);
        merged.productMode = merged.productMode ?? productModeFromTransport(merged.proxyMode, merged.systemProxyMode);
        const storedKey =
          merged.lastSelectedServerKey ??
          (merged.activeServer ? getServerSelectionKey(merged.activeServer) : null);
        const activeServer =
          resolveStoredActiveServer(merged.activeServer, storedKey, merged.servers) ||
          (merged.servers.length === 0 ? merged.activeServer : null);

        return {
          ...merged,
          activeServer,
          lastSelectedServerKey: activeServer ? getServerSelectionKey(activeServer) : storedKey,
        };
      },
    }
  )
);
