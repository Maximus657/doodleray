type BuildChannel = 'direct' | 'store-win32' | 'app-store' | 'internal-qa';

const env = import.meta.env as Record<string, string | undefined>;

export function getBuildChannel(): BuildChannel {
  const explicit = env.VITE_DOODLERAY_BUILD_CHANNEL || env.VITE_DOODLERAY_UPDATE_CHANNEL;
  if (explicit === 'store-win32') return 'store-win32';
  if (explicit === 'app-store') return 'app-store';
  if (explicit === 'internal-qa') return 'internal-qa';
  return 'direct';
}

export function isClosedControlPlaneEnabled(): boolean {
  return env.VITE_DOODLERAY_CLOSED_CONTROL_PLANE === '1';
}

export function isLegacyImportEnabled(): boolean {
  if (!isClosedControlPlaneEnabled()) return true;
  return getBuildChannel() === 'internal-qa' && env.VITE_DOODLERAY_ENABLE_LEGACY_IMPORT === '1';
}

/**
 * Diagnostics telemetry is opt-in at build time. Store and ordinary release
 * builds collect nothing unless this flag is deliberately enabled and the
 * matching disclosure/consent flow is shipped.
 */
export function isDiagnosticsTelemetryEnabled(): boolean {
  return env.VITE_DOODLERAY_DIAGNOSTICS_TELEMETRY === '1';
}

/** LaunchAgent-based autostart is not part of the sandboxed App Store flavor. */
export function isDesktopAutostartAvailable(): boolean {
  return getBuildChannel() !== 'app-store';
}

export function isNetworkExtensionOnlyBuild(): boolean {
  return getBuildChannel() === 'app-store';
}

export function getPrivacyPolicyUrl(): string {
  const value = env.VITE_DOODLERAY_PRIVACY_POLICY_URL?.trim() ?? '';
  return value.startsWith('https://') ? value : '';
}

export function legacyImportDisabledMessage(): string {
  return 'Legacy subscription and proxy-link import is disabled in this build.';
}
