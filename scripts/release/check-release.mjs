import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join } from 'node:path';

const directUpdaterEndpoint = 'https://doodleray.clickflare.click/channels/direct/latest.json';
const directIdentifier = 'com.doodlevpn.doodleray';
const appStoreIdentifier = 'com.doodleray.doodleray';
const packetTunnelIdentifier = 'com.doodleray.doodleray.DoodleRayVPN';
const appGroupIdentifier = 'group.com.doodleray.doodleray';
const windowsBundleResources = {
  'xray-core/*': 'xray-core/',
  'sing-box*': './',
  'wintun*': './',
  'DoodleRayService.exe': 'DoodleRayService.exe',
};
const windowsRuntimeFiles = ['DoodleRayService.exe', 'sing-box.exe', 'wintun.dll', 'xray-core/xray.exe'];
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function readJson(root, relativePath) {
  return JSON.parse(readFileSync(join(root, relativePath), 'utf8'));
}

function readOptionalJson(root, relativePath) {
  const path = join(root, relativePath);
  return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : null;
}

function sameKeys(value, keys) {
  return value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === keys.length
    && keys.every((key) => Object.hasOwn(value, key));
}

function compareNumericIdentifier(left, right) {
  const leftValue = BigInt(left);
  const rightValue = BigInt(right);
  return leftValue === rightValue ? 0 : leftValue > rightValue ? 1 : -1;
}

function compareSemver(left, right) {
  const leftMatch = left.match(semverPattern);
  const rightMatch = right.match(semverPattern);
  if (!leftMatch || !rightMatch) throw new Error('versions must be valid SemVer values');

  for (let index = 1; index <= 3; index += 1) {
    const difference = compareNumericIdentifier(leftMatch[index], rightMatch[index]);
    if (difference) return Math.sign(difference);
  }

  const leftPrerelease = leftMatch[4]?.split('.') ?? [];
  const rightPrerelease = rightMatch[4]?.split('.') ?? [];
  if (!leftPrerelease.length || !rightPrerelease.length) return leftPrerelease.length ? -1 : rightPrerelease.length ? 1 : 0;

  for (let index = 0; index < Math.max(leftPrerelease.length, rightPrerelease.length); index += 1) {
    const leftPart = leftPrerelease[index];
    const rightPart = rightPrerelease[index];
    if (leftPart === undefined) return -1;
    if (rightPart === undefined) return 1;
    if (leftPart === rightPart) continue;
    const leftNumeric = /^\d+$/.test(leftPart);
    const rightNumeric = /^\d+$/.test(rightPart);
    if (leftNumeric && rightNumeric) return compareNumericIdentifier(leftPart, rightPart);
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    return leftPart < rightPart ? -1 : 1;
  }
  return 0;
}

function tomlPackageVersion(cargoToml) {
  return cargoToml.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
}

function settingValues(source, setting) {
  return [...source.matchAll(new RegExp(`\\b${setting}\\s*[:=]\\s*["']?([^"';\\s]+)`, 'g'))].map((match) => match[1]);
}

function absentOrAllEqual(values, expected) {
  return values.every((value) => value === expected);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function hasRustStringConstant(source, name, value) {
  return new RegExp(`\\bconst\\s+${escapeRegExp(name)}\\s*:[^=]+?=\\s*"${escapeRegExp(value)}";`).test(uncomment(source));
}

function resourceStrings(value) {
  if (typeof value === 'string') return [value];
  if (Array.isArray(value)) return value.flatMap(resourceStrings);
  if (value && typeof value === 'object') return Object.entries(value).flatMap(([key, nested]) => [key, ...resourceStrings(nested)]);
  return [];
}

function rustStringArray(source, name) {
  const body = source.match(new RegExp(`\\bconst\\s+${name}\\s*:[\\s\\S]*?=\\s*&\\[([\\s\\S]*?)\\];`))?.[1];
  if (!body) return [];
  const uncommented = body.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
  return [...uncommented.matchAll(/"([^"\\]+)"/g)].map((match) => match[1]);
}

function projectTargetBlock(source, target) {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => line === `  ${target}:`);
  if (start === -1) return '';
  const end = lines.findIndex((line, index) => index > start && /^  \S/.test(line));
  return lines.slice(start, end === -1 ? undefined : end).join('\n');
}

function plistArrayContains(source, key, value) {
  const array = source.replace(/<!--[\s\S]*?-->/g, '').match(new RegExp(`<key>${escapeRegExp(key)}</key>\\s*<array>([\\s\\S]*?)</array>`))?.[1];
  return array !== undefined && new RegExp(`<string>\\s*${escapeRegExp(value)}\\s*</string>`).test(array);
}

function uncomment(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
}

function nsisPostinstallRequiredFiles(source) {
  const body = source.match(/!macro NSIS_HOOK_POSTINSTALL\b([\s\S]*?)!macroend/)?.[1] ?? '';
  return [...body.matchAll(/^\s*!insertmacro\s+DoodleRayRequireFile\s+"([^"]+)"/gm)].map((match) => match[1]);
}

export function checkRelease(root, { publishedVersion } = {}) {
  const errors = [];
  const release = readJson(root, 'release/release.json');
  if (!sameKeys(release, ['version', 'macBuild', 'channel', 'targets'])) errors.push('release.json must contain exactly version, macBuild, channel, and targets');
  if (!semverPattern.test(release.version ?? '')) errors.push('release.json version must be valid SemVer');
  if (!Number.isSafeInteger(release.macBuild) || release.macBuild <= 0) errors.push('release.json macBuild must be a positive integer');
  if (release.channel !== 'stable') errors.push('release.json channel must be stable');
  if (!sameKeys(release.targets, ['windows', 'macAppStore']) || typeof release.targets.windows !== 'boolean' || typeof release.targets.macAppStore !== 'boolean') {
    errors.push('release.json targets must contain exactly boolean windows and macAppStore values');
  } else if (!release.targets.windows && !release.targets.macAppStore) {
    errors.push('at least one release target must be enabled');
  }

  const packageJson = readJson(root, 'package.json');
  const packageLock = readJson(root, 'package-lock.json');
  const tauri = readJson(root, 'src-tauri/tauri.conf.json');
  const macos = readOptionalJson(root, 'src-tauri/tauri.macos.conf.json');
  const appStore = readJson(root, 'src-tauri/tauri.appstore.conf.json');
  const windows = readJson(root, 'src-tauri/tauri.windows.conf.json');
  const cargoVersion = tomlPackageVersion(readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8'));
  const projectYml = readFileSync(join(root, 'src-tauri/macos/project.yml'), 'utf8');
  const pbxproj = readFileSync(join(root, 'src-tauri/macos/DoodleRayAppStoreExtensions.xcodeproj/project.pbxproj'), 'utf8');
  const appStoreBuild = readFileSync(join(root, 'scripts/macos/build-app-store.sh'), 'utf8');
  const hostEntitlements = readFileSync(join(root, 'src-tauri/Entitlements.appstore.plist'), 'utf8');
  const extensionEntitlements = readFileSync(join(root, 'src-tauri/macos/PacketTunnelProvider/Entitlements.plist'), 'utf8');
  const extensionBridge = readFileSync(join(root, 'src-tauri/macos/HostBridge/NetworkExtensionBridge.m'), 'utf8');
  const tunnelService = readFileSync(join(root, 'src-tauri/src/tunnel_service.rs'), 'utf8');
  const secureStore = readFileSync(join(root, 'src-tauri/src/storage/mod.rs'), 'utf8');
  const buildConfig = readFileSync(join(root, 'src-tauri/build_config.rs'), 'utf8');
  const nsisHooks = readFileSync(join(root, 'src-tauri/nsis-hooks.nsh'), 'utf8');

  if (packageJson.version !== release.version) errors.push('package.json version must equal release.json version');
  if (packageLock.version !== release.version || packageLock.packages?.['']?.version !== release.version) errors.push('package-lock.json root version must equal release.json version');
  if (cargoVersion !== release.version) errors.push('Cargo package version must equal release.json version');
  if (tauri.version !== release.version) errors.push('base Tauri version must equal release.json version');
  if (appStore.bundle?.macOS?.bundleVersion !== undefined
    && appStore.bundle.macOS.bundleVersion !== String(release.macBuild)) errors.push('App Store bundleVersion must equal release.json macBuild when declared');
  if (!absentOrAllEqual(settingValues(projectYml, 'MARKETING_VERSION'), release.version)) errors.push('XcodeGen MARKETING_VERSION must equal release.json version when declared');
  if (!absentOrAllEqual(settingValues(projectYml, 'CURRENT_PROJECT_VERSION'), String(release.macBuild))) errors.push('XcodeGen CURRENT_PROJECT_VERSION must equal release.json macBuild when declared');
  if (!absentOrAllEqual(settingValues(pbxproj, 'MARKETING_VERSION'), release.version)) errors.push('generated pbxproj MARKETING_VERSION must equal release.json version when declared');
  if (!absentOrAllEqual(settingValues(pbxproj, 'CURRENT_PROJECT_VERSION'), String(release.macBuild))) errors.push('generated pbxproj CURRENT_PROJECT_VERSION must equal release.json macBuild when declared');
  if (!/--config\s+"\$release_config"/.test(appStoreBuild)
    || !/MARKETING_VERSION="\$RELEASE_VERSION"/.test(appStoreBuild)
    || !/CURRENT_PROJECT_VERSION="\$RELEASE_BUILD"/.test(appStoreBuild)) {
    errors.push('App Store build must inject release.json version and macBuild into Tauri and Xcode');
  }
  if (tauri.identifier !== directIdentifier) errors.push(`base Tauri identifier must equal ${directIdentifier}`);
  if (appStore.identifier !== appStoreIdentifier) errors.push(`App Store identifier must equal ${appStoreIdentifier}`);
  const packetTunnelProject = projectTargetBlock(projectYml, 'DoodleRayVPN');
  const activeBridge = uncomment(extensionBridge);
  if (!new RegExp(`^        PRODUCT_BUNDLE_IDENTIFIER:[ \\t]*${escapeRegExp(packetTunnelIdentifier)}[ \\t]*$`, 'm').test(packetTunnelProject)
    || !new RegExp(`\\bstatic\\s+NSString\\s*\\*\\s*const\\s+DoodleRayProviderBundleIdentifier\\s*=\\s*@"${escapeRegExp(packetTunnelIdentifier)}"\\s*;`).test(activeBridge)) {
    errors.push(`Packet Tunnel identifier must equal ${packetTunnelIdentifier} in project and bridge`);
  }
  if (!new RegExp(`^        com\\.apple\\.security\\.application-groups:[ \\t]*\\r?\\n          - ${escapeRegExp(appGroupIdentifier)}[ \\t]*$`, 'm').test(packetTunnelProject)
    || !plistArrayContains(hostEntitlements, 'com.apple.security.application-groups', appGroupIdentifier)
    || !plistArrayContains(extensionEntitlements, 'com.apple.security.application-groups', appGroupIdentifier)
    || !new RegExp(`\\bcontainerURLForSecurityApplicationGroupIdentifier:\\s*@"${escapeRegExp(appGroupIdentifier)}"\\s*\\]`).test(activeBridge)) {
    errors.push(`App Group must equal ${appGroupIdentifier} in project, host, extension, and bridge`);
  }
  if (!hasRustStringConstant(tunnelService, 'TUNNEL_SERVICE_NAME', 'DoodleRayTunnelService') || !hasRustStringConstant(tunnelService, 'TUNNEL_SERVICE_DISPLAY_NAME', 'DoodleRay Tunnel Service')) {
    errors.push('Windows service name/display name must equal DoodleRayTunnelService / DoodleRay Tunnel Service');
  }
  if (!hasRustStringConstant(secureStore, 'SECURE_STORE_SERVICE', 'DoodleRay')
    || !hasRustStringConstant(secureStore, 'RENDERER_STATE_KEY', 'doodleray-storage')
    || !hasRustStringConstant(secureStore, 'APP_API_SESSION_KEY', 'app-api-session-v1')
    || !hasRustStringConstant(secureStore, 'APP_API_DEVICE_KEY', 'app-api-device-v1')) {
    errors.push('secure-store service and keys must retain the compatibility contract');
  }
  if (appStore.bundle?.createUpdaterArtifacts !== false) errors.push('App Store overlay createUpdaterArtifacts must be false');
  if ([tauri, macos, appStore].flatMap((config) => resourceStrings(config?.bundle?.resources)).some((resource) => /(?:^|[/\\])(?:xray(?:-core)?|sing-box)(?:\.exe)?(?:$|[/\\*])/.test(resource))) {
    errors.push('App Store bundle resources must not include direct engine executables');
  }
  if (typeof tauri.plugins?.updater?.pubkey !== 'string' || !tauri.plugins.updater.pubkey.trim()) errors.push('base updater public key must be non-empty');
  if (JSON.stringify(tauri.plugins?.updater?.endpoints) !== JSON.stringify([directUpdaterEndpoint])) errors.push('base updater endpoints must equal the direct HTTPS endpoint');
  if (!Object.entries(windowsBundleResources).every(([source, destination]) => windows.bundle?.resources?.[source] === destination)
    || !windowsRuntimeFiles.every((file) => rustStringArray(buildConfig, 'WINDOWS_RUNTIME_FILES').includes(file))) {
    errors.push('Windows bundle resources must match the runtime inventory');
  }
  const nsisRequiredFiles = nsisPostinstallRequiredFiles(nsisHooks);
  const requiredNsisFiles = ['DoodleRayService.exe', 'sing-box.exe', 'wintun.dll', 'xray-core\\xray.exe'];
  if (nsisRequiredFiles.length !== requiredNsisFiles.length || !requiredNsisFiles.every((file) => nsisRequiredFiles.includes(file))) {
    errors.push('NSIS required-file inventory must match the Windows bundle contract');
  }
  if (publishedVersion !== undefined) {
    if (!semverPattern.test(publishedVersion)) errors.push('published version must be valid SemVer');
    else if (semverPattern.test(release.version ?? '') && compareSemver(release.version, publishedVersion) <= 0) {
      errors.push(`release version ${release.version} must be strictly newer than published version ${publishedVersion}`);
    }
  }

  if (errors.length) throw new Error(`Release preflight failed: ${errors.join('; ')}`);
  return release;
}

function parseCliArguments(arguments_) {
  if (!arguments_.length) return {};
  if (arguments_.length === 2 && arguments_[0] === '--published-version') return { publishedVersion: arguments_[1] };
  throw new Error('usage: node scripts/release/check-release.mjs [--published-version X.Y.Z]');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const release = checkRelease(fileURLToPath(new URL('../../', import.meta.url)), parseCliArguments(process.argv.slice(2)));
    console.log(`Release metadata preflight passed for ${release.version} (macOS build ${release.macBuild}).`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
