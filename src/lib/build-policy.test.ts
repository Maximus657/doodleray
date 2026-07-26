import {
  getBuildChannel,
  isClosedControlPlaneEnabled,
  isDesktopAutostartAvailable,
  isLegacyImportEnabled,
  isNetworkExtensionOnlyBuild,
} from './build-policy.ts';
import {
  getStoreUpdateFallbackUrl,
  getUpdateChannel,
  isInAppUpdateEnabled,
  isUpdateManagedByStore,
} from './update-channel.ts';

function assertEqual(actual: unknown, expected: unknown) {
  if (actual !== expected) throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
}

const direct = {};
assertEqual(getBuildChannel(direct), 'direct');
assertEqual(getUpdateChannel(direct), 'direct');
assertEqual(isClosedControlPlaneEnabled(direct), true);
assertEqual(isLegacyImportEnabled(direct), false);
assertEqual(isDesktopAutostartAvailable(direct), true);
assertEqual(isNetworkExtensionOnlyBuild(direct), false);
assertEqual(isUpdateManagedByStore(direct), false);
assertEqual(isInAppUpdateEnabled(direct), true);

const appStore = {
  VITE_DOODLERAY_BUILD_CHANNEL: 'app-store',
  VITE_DOODLERAY_UPDATE_CHANNEL: 'app-store',
  VITE_DOODLERAY_STORE_SELF_UPDATE: '1',
};
assertEqual(getBuildChannel(appStore), 'app-store');
assertEqual(getUpdateChannel(appStore), 'app-store');
assertEqual(isLegacyImportEnabled(appStore), false);
assertEqual(isDesktopAutostartAvailable(appStore), false);
assertEqual(isNetworkExtensionOnlyBuild(appStore), true);
assertEqual(isUpdateManagedByStore(appStore), true);
assertEqual(isInAppUpdateEnabled(appStore), false);
assertEqual(getStoreUpdateFallbackUrl(appStore), 'macappstore://showUpdatesPage');

const internalQa = {
  VITE_DOODLERAY_BUILD_CHANNEL: 'internal-qa',
  VITE_DOODLERAY_UPDATE_CHANNEL: 'direct',
  VITE_DOODLERAY_ENABLE_LEGACY_IMPORT: '1',
};
assertEqual(getBuildChannel(internalQa), 'internal-qa');
assertEqual(getUpdateChannel(internalQa), 'direct');
assertEqual(isClosedControlPlaneEnabled(internalQa), true);
assertEqual(isLegacyImportEnabled(internalQa), true);
assertEqual(isDesktopAutostartAvailable(internalQa), true);
assertEqual(isNetworkExtensionOnlyBuild(internalQa), false);
assertEqual(isUpdateManagedByStore(internalQa), false);
assertEqual(isInAppUpdateEnabled(internalQa), true);

const retiredStoreWin32 = {
  VITE_DOODLERAY_BUILD_CHANNEL: 'store-win32',
  VITE_DOODLERAY_UPDATE_CHANNEL: 'store-win32',
  VITE_DOODLERAY_STORE_SELF_UPDATE: '0',
};
assertEqual(getBuildChannel(retiredStoreWin32), 'direct');
assertEqual(getUpdateChannel(retiredStoreWin32), 'direct');
assertEqual(isInAppUpdateEnabled(retiredStoreWin32), true);

console.log('build policy tests passed');
