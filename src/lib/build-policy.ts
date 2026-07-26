type BuildChannel = 'direct' | 'app-store' | 'internal-qa';
type BuildEnvironment = Readonly<Record<string, string | undefined>>;

const env = (import.meta.env ?? {}) as BuildEnvironment;

export function getBuildChannel(source: BuildEnvironment = env): BuildChannel {
  const explicit = source.VITE_DOODLERAY_BUILD_CHANNEL || source.VITE_DOODLERAY_UPDATE_CHANNEL;
  if (explicit === 'app-store') return 'app-store';
  if (explicit === 'internal-qa') return 'internal-qa';
  return 'direct';
}

export function isClosedControlPlaneEnabled(source: BuildEnvironment = env): boolean {
  return source.VITE_DOODLERAY_CLOSED_CONTROL_PLANE !== '0';
}

export function isLegacyImportEnabled(source: BuildEnvironment = env): boolean {
  if (!isClosedControlPlaneEnabled(source)) return true;
  return getBuildChannel(source) === 'internal-qa' && source.VITE_DOODLERAY_ENABLE_LEGACY_IMPORT === '1';
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
export function isDesktopAutostartAvailable(source: BuildEnvironment = env): boolean {
  return getBuildChannel(source) !== 'app-store';
}

export function isNetworkExtensionOnlyBuild(source: BuildEnvironment = env): boolean {
  return getBuildChannel(source) === 'app-store';
}

export function getPrivacyPolicyUrl(): string {
  const value = env.VITE_DOODLERAY_PRIVACY_POLICY_URL?.trim() ?? '';
  return value.startsWith('https://') ? value : '';
}

export function legacyImportDisabledMessage(): string {
  return 'Legacy subscription and proxy-link import is disabled in this build.';
}
