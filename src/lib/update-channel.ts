/**
 * Distribution-channel policy for app updates.
 *
 * Baked at build time via Vite env vars:
 * - VITE_DOODLERAY_UPDATE_CHANNEL: 'direct' (default) | 'app-store'
 * - VITE_DOODLERAY_STORE_FALLBACK_URL: App Store page opened instead of
 *   self-update for the managed macOS build.
 *
 * Direct Windows builds keep the existing signed Tauri updater behavior.
 */
export type UpdateChannel = 'direct' | 'app-store';
type UpdateEnvironment = Readonly<Record<string, string | undefined>>;

const env = (import.meta.env ?? {}) as UpdateEnvironment;

export function getUpdateChannel(source: UpdateEnvironment = env): UpdateChannel {
  return source.VITE_DOODLERAY_UPDATE_CHANNEL === 'app-store' ? 'app-store' : 'direct';
}

/** Apple owns update discovery and installation for Mac App Store builds. */
export function isUpdateManagedByStore(source: UpdateEnvironment = env): boolean {
  return getUpdateChannel(source) === 'app-store';
}

/** True when the build may download+install updates in-app (signed, user-initiated). */
export function isInAppUpdateEnabled(source: UpdateEnvironment = env): boolean {
  return getUpdateChannel(source) === 'direct';
}

export function getStoreUpdateFallbackUrl(source: UpdateEnvironment = env): string {
  return source.VITE_DOODLERAY_STORE_FALLBACK_URL || 'macappstore://showUpdatesPage';
}

/** Open the Mac App Store updates page for the managed macOS build. */
export async function openStoreUpdatePage(): Promise<void> {
  const url = getStoreUpdateFallbackUrl();
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch {
    window.open(url, '_blank');
  }
}
