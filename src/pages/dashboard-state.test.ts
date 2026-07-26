import { shallow } from 'zustand/vanilla/shallow';
import type { AppState } from '../stores/app-store.ts';
import { selectDashboardState } from './dashboard-state.ts';

export {};

const action = () => {};
const baseState = {
  status: 'disconnected',
  setStatus: action,
  activeServer: null,
  servers: [],
  setActiveServer: action,
  proxyMode: 'tun',
  setProxyMode: action,
  systemProxyMode: 'set',
  setSystemProxyMode: action,
  productMode: 'protected',
  setRequestModeSwitch: action,
  currentDownload: 0,
  currentUpload: 0,
  addTraffic: action,
  resetTraffic: action,
  addSpeedPoint: action,
  setCurrentSpeed: action,
  logs: [],
  addLog: action,
  clearLogs: action,
  socksPort: 10808,
  httpPort: 10809,
  subscriptions: [],
  updateSubscription: action,
  autoSelectFastest: false,
  subAutoUpdateMinutes: 30,
  connectedAt: null,
  setConnectedAt: action,
  addSubscription: action,
  addServer: action,
  setSocksPort: action,
  setHttpPort: action,
  showStats: true,
  appSessionDeviceAllowed: null,
  language: 'ru',
  theme: 'dark',
} as unknown as AppState;

const selected = selectDashboardState(baseState);
const afterUnrelatedUpdate = selectDashboardState({ ...baseState, theme: 'light' });
if (!shallow(selected, afterUnrelatedUpdate)) {
  throw new Error('Unrelated store updates must keep the Dashboard selection shallow-equal');
}

const afterSelectedUpdate = selectDashboardState({ ...baseState, status: 'connected' });
if (shallow(selected, afterSelectedUpdate)) {
  throw new Error('Selected store updates must change the Dashboard selection');
}

if ('theme' in selected) throw new Error('Dashboard selection must not include unrelated theme state');

console.log('dashboard state selector tests passed');
