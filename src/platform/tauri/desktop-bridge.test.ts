import { DesktopBridge } from './desktop-bridge.ts';

export {};

function assertEqual(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

function assertJsonEqual(actual: unknown, expected: unknown) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) throw new Error(`Expected ${expectedJson}, got ${actualJson}`);
}

const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const values = new Map<string, unknown>([
  ['app_api_session_status', { logged_in: false }],
  ['app_api_exchange_code', { logged_in: true, device_id: 'device-1' }],
  ['app_api_exchange_legacy_subscription', { logged_in: true, device_id: 'legacy-device' }],
  ['app_api_refresh', { logged_in: true, device_id: 'refreshed-device' }],
  ['app_api_locations', { locations: [{ id: 'de' }] }],
  ['app_api_subscription_status', { active: true }],
  ['vpn_disconnect', 'disconnected'],
  ['prepare_for_app_update', 'prepared'],
  ['get_connection_health', { verdict: 'protected', checks: [] }],
  ['check_defender_exclusion', true],
  ['add_defender_exclusion', 'added'],
  ['toggle_silent_autostart', 'Silent autostart enabled'],
  ['is_admin', false],
]);
const invokeCommand = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  calls.push({ command, args });
  return values.get(command) as T;
};
const bridge = new DesktopBridge(invokeCommand);

assertJsonEqual(await bridge.appApiSessionStatus(), { logged_in: false });
assertJsonEqual(await bridge.appApiExchangeCode('12345678'), { logged_in: true, device_id: 'device-1' });
assertJsonEqual(
  await bridge.appApiExchangeLegacySubscription('https://example.invalid/sub/token'),
  { logged_in: true, device_id: 'legacy-device' },
);
assertJsonEqual(await bridge.appApiRefresh(), { logged_in: true, device_id: 'refreshed-device' });
assertEqual(await bridge.appApiLogout(), undefined);
assertJsonEqual(await bridge.appApiLocations(), { locations: [{ id: 'de' }] });
assertJsonEqual(await bridge.appApiSubscriptionStatus(), { active: true });
assertEqual(await bridge.vpnDisconnect(), 'disconnected');
assertEqual(await bridge.prepareForAppUpdate(), 'prepared');
assertEqual(await bridge.secureStoreSet('doodleray-storage', 'state'), undefined);
assertEqual(await bridge.secureStoreDelete('doodleray-storage'), undefined);
assertJsonEqual(
  await bridge.getConnectionHealth('tun', 'set', 10808, 10809),
  { verdict: 'protected', checks: [] },
);
assertEqual(await bridge.checkDefenderExclusion(), true);
assertEqual(await bridge.addDefenderExclusion(), 'added');
const toggleResult: string = await bridge.toggleSilentAutostart(true);
assertEqual(toggleResult, 'Silent autostart enabled');
assertEqual(await bridge.isAdmin(), false);
assertEqual(await bridge.restartAsAdmin(), undefined);

assertJsonEqual(calls, [
  { command: 'app_api_session_status' },
  { command: 'app_api_exchange_code', args: { request: { code: '12345678' } } },
  {
    command: 'app_api_exchange_legacy_subscription',
    args: { request: { subscription_url: 'https://example.invalid/sub/token' } },
  },
  { command: 'app_api_refresh' },
  { command: 'app_api_logout' },
  { command: 'app_api_locations' },
  { command: 'app_api_subscription_status' },
  { command: 'vpn_disconnect' },
  { command: 'prepare_for_app_update' },
  { command: 'secure_store_set', args: { key: 'doodleray-storage', value: 'state' } },
  { command: 'secure_store_delete', args: { key: 'doodleray-storage' } },
  {
    command: 'get_connection_health',
    args: { proxyMode: 'tun', systemProxyMode: 'set', socksPort: 10808, httpPort: 10809 },
  },
  { command: 'check_defender_exclusion' },
  { command: 'add_defender_exclusion' },
  { command: 'toggle_silent_autostart', args: { enable: true } },
  { command: 'is_admin' },
  { command: 'restart_as_admin' },
]);

const expectedError = new Error('invoke failed');
const rejectingBridge = new DesktopBridge(async () => {
  throw expectedError;
});
let receivedError: unknown;
try {
  await rejectingBridge.prepareForAppUpdate();
} catch (error) {
  receivedError = error;
}
assertEqual(receivedError, expectedError);

console.log('desktop bridge tests passed');
