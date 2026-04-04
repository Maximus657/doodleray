import { create } from 'zustand';
import { persist } from 'zustand/middleware';
// Trigger HMR

// ========== Types ==========

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected';

export type ProxyMode = 'system-proxy' | 'tun' | 'vpn';

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
}

export interface SpeedPoint {
  time: string;
  download: number;
  upload: number;
}

export interface AppState {
  status: ConnectionStatus;
  activeServer: ServerConfig | null;
  proxyMode: ProxyMode;

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
  language: 'ru' | 'en' | 'zh';
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
  showStats: boolean; // Hide/show statistics on dashboard

  setStatus: (status: ConnectionStatus) => void;
  setActiveServer: (server: ServerConfig | null) => void;
  setProxyMode: (mode: ProxyMode) => void;

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
  addSpeedPoint: (point: SpeedPoint) => void;
  setCurrentSpeed: (download: number, upload: number) => void;
  setSocksPort: (port: number) => void;
  setHttpPort: (port: number) => void;
  setTheme: (theme: 'dark' | 'light') => void;
  setLanguage: (lang: 'ru' | 'en' | 'zh') => void;
  addLog: (level: LogEntry['level'], message: string) => void;
  clearLogs: () => void;
  wipeData: () => void;
  setAvailableUpdate: (version: string | null) => void;
  addTraffic: (dl: number, ul: number) => void;
  resetTraffic: () => void;
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      status: 'disconnected',
      activeServer: null,
      proxyMode: 'vpn' as ProxyMode,

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
      autoStart: false,
      silentAdminAutostart: false,
      theme: 'dark',
      language: 'en',
      networkStack: 'mixed',
      dnsMode: 'fakeip',
      strictRoute: true,
      killSwitch: false,
      autoSelectFastest: true,
      subAutoUpdateMinutes: 60,
      connectedAt: null,
      alwaysRunAdmin: false,
      autoConnectOnStartup: false,
      availableUpdate: null,
      showStats: false,

      setStatus: (status) => set({ status }),
      setActiveServer: (server) => set({ activeServer: server }),
      setProxyMode: (mode) => set({ proxyMode: mode }),

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
      removeServer: (id) => set((s) => ({
        servers: s.servers.filter((s2) => s2.id !== id),
        activeServer: s.activeServer?.id === id ? null : s.activeServer
      })),
      removeAllManualServers: () => set((s) => ({
        servers: s.servers.filter((srv) => srv.subscriptionId !== undefined),
        activeServer: (!s.activeServer?.subscriptionId) ? null : s.activeServer
      })),
      setServers: (servers) => set({ servers }),

      addSubscription: (sub) => set((s) => {
        // If subscription with same URL already exists, replace it
        const existing = s.subscriptions.find((x) => x.url === sub.url);
        const newSubscriptions = existing
          ? s.subscriptions.map((x) => x.url === sub.url ? sub : x)
          : [...s.subscriptions, sub];
        const newServers = existing
          ? [...s.servers.filter((srv) => srv.subscriptionId !== existing.id), ...sub.servers]
          : [...s.servers, ...sub.servers];
        // Deduplicate servers by address+port+protocol
        const seen = new Set<string>();
        const deduped = newServers.filter((srv) => {
          const key = `${srv.address}:${srv.port}:${srv.protocol}:${srv.name}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        });
        return { subscriptions: newSubscriptions, servers: deduped };
      }),
      removeSubscription: (id) => set((s) => ({
        subscriptions: s.subscriptions.filter((sub) => sub.id !== id),
        servers: s.servers.filter((srv) => srv.subscriptionId !== id),
        activeServer: s.activeServer?.subscriptionId === id ? null : s.activeServer
      })),
      updateSubscription: (id, newSub) => set((s) => {
        const newServers = [
          ...s.servers.filter((srv) => srv.subscriptionId !== id),
          ...newSub.servers,
        ];
        // Deduplicate
        const seen = new Set<string>();
        const deduped = newServers.filter((srv) => {
          const key = `${srv.address}:${srv.port}:${srv.protocol}:${srv.name}`;
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        });
        return {
          subscriptions: s.subscriptions.map((sub) => sub.id === id ? newSub : sub),
          servers: deduped,
        };
      }),

      updateServerPing: (id, ping) => set((s) => ({
        servers: s.servers.map((srv) =>
          srv.id === id ? { ...srv, ping } : srv
        ),
        activeServer: s.activeServer?.id === id ? { ...s.activeServer, ping } : s.activeServer
      })),

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
      wipeData: () => set({ servers: [], subscriptions: [], activeServer: null }),
      setAvailableUpdate: (version) => set({ availableUpdate: version }),
      addTraffic: (dl, ul) => set((s) => ({ totalDown: s.totalDown + dl, totalUp: s.totalUp + ul })),
      resetTraffic: () => set({ totalDown: 0, totalUp: 0, speedHistory: [], currentDownload: 0, currentUpload: 0 }),
    }),
    {
      name: 'doodleray-storage',
      partialize: (state) => Object.fromEntries(
        Object.entries(state as any).filter(([key]) => !['status', 'speedHistory', 'currentDownload', 'currentUpload', 'totalDown', 'totalUp', 'logs', 'availableUpdate'].includes(key))
      ) as Partial<AppState>,
    }
  )
);
