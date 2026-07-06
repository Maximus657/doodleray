type BuildChannel = 'direct' | 'store-win32' | 'internal-qa';

const env = import.meta.env as Record<string, string | undefined>;

export function getBuildChannel(): BuildChannel {
  const explicit = env.VITE_DOODLERAY_BUILD_CHANNEL || env.VITE_DOODLERAY_UPDATE_CHANNEL;
  if (explicit === 'store-win32') return 'store-win32';
  if (explicit === 'internal-qa') return 'internal-qa';
  return 'direct';
}

export function isClosedControlPlaneEnabled(): boolean {
  return env.VITE_DOODLERAY_CLOSED_CONTROL_PLANE !== '0';
}

export function isLegacyImportEnabled(): boolean {
  if (!isClosedControlPlaneEnabled()) return true;
  return getBuildChannel() === 'internal-qa' && env.VITE_DOODLERAY_ENABLE_LEGACY_IMPORT === '1';
}

export function legacyImportDisabledMessage(): string {
  return 'Legacy subscription and proxy-link import is disabled in this build.';
}
