import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join } from 'node:path';

const directUpdaterEndpoint = 'https://doodleray.clickflare.click/channels/direct/latest.json';
const semverPattern = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function readJson(root, relativePath) {
  return JSON.parse(readFileSync(join(root, relativePath), 'utf8'));
}

function sameKeys(value, keys) {
  return value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).length === keys.length
    && keys.every((key) => Object.hasOwn(value, key));
}

function compareSemver(left, right) {
  const leftMatch = left.match(semverPattern);
  const rightMatch = right.match(semverPattern);
  if (!leftMatch || !rightMatch) throw new Error('versions must be valid SemVer values');

  for (let index = 1; index <= 3; index += 1) {
    const difference = Number(leftMatch[index]) - Number(rightMatch[index]);
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
    if (leftNumeric && rightNumeric) return Math.sign(Number(leftPart) - Number(rightPart));
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

function allEqual(values, expected) {
  return values.length > 0 && values.every((value) => value === expected);
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
  }

  const packageJson = readJson(root, 'package.json');
  const packageLock = readJson(root, 'package-lock.json');
  const tauri = readJson(root, 'src-tauri/tauri.conf.json');
  const appStore = readJson(root, 'src-tauri/tauri.appstore.conf.json');
  const cargoVersion = tomlPackageVersion(readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8'));
  const projectYml = readFileSync(join(root, 'src-tauri/macos/project.yml'), 'utf8');
  const pbxproj = readFileSync(join(root, 'src-tauri/macos/DoodleRayAppStoreExtensions.xcodeproj/project.pbxproj'), 'utf8');

  if (packageJson.version !== release.version) errors.push('package.json version must equal release.json version');
  if (packageLock.version !== release.version || packageLock.packages?.['']?.version !== release.version) errors.push('package-lock.json root version must equal release.json version');
  if (cargoVersion !== release.version) errors.push('Cargo package version must equal release.json version');
  if (tauri.version !== release.version) errors.push('base Tauri version must equal release.json version');
  if (appStore.bundle?.macOS?.bundleVersion !== String(release.macBuild)) errors.push('App Store bundleVersion must equal release.json macBuild');
  if (!allEqual(settingValues(projectYml, 'MARKETING_VERSION'), release.version)) errors.push('XcodeGen MARKETING_VERSION must equal release.json version');
  if (!allEqual(settingValues(projectYml, 'CURRENT_PROJECT_VERSION'), String(release.macBuild))) errors.push('XcodeGen CURRENT_PROJECT_VERSION must equal release.json macBuild');
  if (!allEqual(settingValues(pbxproj, 'MARKETING_VERSION'), release.version)) errors.push('generated pbxproj MARKETING_VERSION must equal release.json version');
  if (!allEqual(settingValues(pbxproj, 'CURRENT_PROJECT_VERSION'), String(release.macBuild))) errors.push('generated pbxproj CURRENT_PROJECT_VERSION must equal release.json macBuild');
  if (appStore.bundle?.createUpdaterArtifacts !== false) errors.push('App Store overlay createUpdaterArtifacts must be false');
  if (typeof tauri.plugins?.updater?.pubkey !== 'string' || !tauri.plugins.updater.pubkey.trim()) errors.push('base updater public key must be non-empty');
  if (JSON.stringify(tauri.plugins?.updater?.endpoints) !== JSON.stringify([directUpdaterEndpoint])) errors.push('base updater endpoints must equal the direct HTTPS endpoint');
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
