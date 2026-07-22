/**
 * Distribution-channel policy for app updates (v6 Store track).
 *
 * Baked at build time via Vite env vars (set by scripts/build-store.ps1):
 * - VITE_DOODLERAY_UPDATE_CHANNEL: 'direct' (default) | 'store-win32' | 'app-store'
 * - VITE_DOODLERAY_STORE_SELF_UPDATE: '1' enables signed in-app updates for
 *   the store-win32 channel (requires updater artifacts for that channel);
 *   anything else disables self-update and routes users to the Store page.
 * - VITE_DOODLERAY_STORE_FALLBACK_URL: page opened instead of self-update
 *   (Microsoft Store PDP once listed; support page until then).
 *
 * Policy is config-driven so switching Store update strategy needs no code
 * surgery. The direct channel keeps the existing GitHub updater behavior.
 */
export type UpdateChannel = 'direct' | 'store-win32' | 'app-store';

const env = import.meta.env as Record<string, string | undefined>;

export function getUpdateChannel(): UpdateChannel {
  if (env.VITE_DOODLERAY_UPDATE_CHANNEL === 'app-store') return 'app-store';
  return env.VITE_DOODLERAY_UPDATE_CHANNEL === 'store-win32' ? 'store-win32' : 'direct';
}

/** Apple owns update discovery and installation for Mac App Store builds. */
export function isUpdateManagedByStore(): boolean {
  return getUpdateChannel() === 'app-store';
}

/** True when the build may download+install updates in-app (signed, user-initiated). */
export function isInAppUpdateEnabled(): boolean {
  if (getUpdateChannel() === 'direct') return true;
  if (getUpdateChannel() === 'app-store') return false;
  return env.VITE_DOODLERAY_STORE_SELF_UPDATE === '1';
}

export function getStoreUpdateFallbackUrl(): string {
  if (getUpdateChannel() === 'app-store') {
    return env.VITE_DOODLERAY_STORE_FALLBACK_URL || 'macappstore://showUpdatesPage';
  }
  // TODO(store): replace default with ms-windows-store://pdp/?productid=<ID> after listing.
  return env.VITE_DOODLERAY_STORE_FALLBACK_URL || 'https://t.me/doodlevpn_support';
}

/** Open the Store/support page for user-initiated updates when self-update is off. */
export async function openStoreUpdatePage(): Promise<void> {
  const url = getStoreUpdateFallbackUrl();
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch {
    window.open(url, '_blank');
  }
}
