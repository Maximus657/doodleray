import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const expectedWorkflows = ['attach-app-store-version-macos.yml', 'attach-testflight-macos.yml', 'ci.yml', 'release-production.yml', 'runtime-updates.yml', 'testflight-macos.yml'];
const forbiddenWindowsSigning = /WINDOWS_CODESIGN|PFX|THUMBPRINT|AUTHENTICODE|sign-windows-if-configured|sign-all-pe|Get-AuthenticodeSignature|signCommand/i;
const appleSecrets = [
  'APPLE_DISTRIBUTION_CERTIFICATE_BASE64',
  'APPLE_DISTRIBUTION_CERTIFICATE_PASSWORD',
  'MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_BASE64',
  'MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_PASSWORD',
  'MACOS_APP_STORE_HOST_PROFILE_BASE64',
  'MACOS_APP_STORE_EXTENSION_PROFILE_BASE64',
  'APPLE_TEAM_ID',
  'APP_STORE_CONNECT_API_KEY_ID',
  'APP_STORE_CONNECT_ISSUER_ID',
  'APP_STORE_CONNECT_PRIVATE_KEY',
];

function read(root, relativePath) {
  const path = join(root, relativePath);
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function jobBlocks(workflow) {
  const matches = [...workflow.matchAll(/^  ([a-zA-Z0-9_-]+):\r?\n([\s\S]*?)(?=^  [a-zA-Z0-9_-]+:\r?\n|(?![\s\S]))/gm)];
  return new Map(matches.map((match) => [match[1], match[2]]));
}

export function checkReleaseWorkflows(root) {
  const errors = [];
  const workflowDir = join(root, '.github/workflows');
  const workflows = readdirSync(workflowDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (JSON.stringify(workflows) !== JSON.stringify(expectedWorkflows)) {
    errors.push(`active workflows must be exactly ${expectedWorkflows.join(', ')}`);
  }

  const release = read(root, '.github/workflows/release-production.yml');
  const ci = read(root, '.github/workflows/ci.yml');
  const runtime = read(root, '.github/workflows/runtime-updates.yml');
  const testflight = read(root, '.github/workflows/testflight-macos.yml');
  const attachAppStore = read(root, '.github/workflows/attach-app-store-version-macos.yml');
  const prepareWindows = read(root, 'scripts/release/Prepare-WindowsRelease.ps1');
  const publishWindows = read(root, 'scripts/release/Publish-DoodleRayDownloads.ps1');
  const updaterVerifier = read(root, 'src-tauri/examples/verify_updater_signature.rs');
  const cargoToml = read(root, 'src-tauri/Cargo.toml');
  const activeWindowsSources = [
    release,
    ci,
    runtime,
    testflight,
    read(root, 'src-tauri/tauri.windows.conf.json'),
    publishWindows,
  ].join('\n');
  if (forbiddenWindowsSigning.test(activeWindowsSources)) errors.push('active release paths must not contain Windows Authenticode prerequisites');

  if (!/^\s*workflow_dispatch:/m.test(release)) errors.push('production release must use workflow_dispatch');
  if (!/source_sha:/m.test(release) || !/dry_run:/m.test(release)) errors.push('production dispatch must require source_sha and expose dry_run');
  if (!/group:\s*release-production/m.test(release) || !/cancel-in-progress:\s*false/m.test(release)) errors.push('production release concurrency contract is missing');
  if (!/^permissions:\r?\n\s+contents:\s+read/m.test(release)) errors.push('production workflow must default to contents: read');
  if (!/github\.ref[\s\S]{0,80}refs\/heads\/main/.test(release)
    || !/merge-base --is-ancestor/.test(release)
    || !/rev-parse (?:refs\/remotes\/)?origin\/main/.test(release)) {
    errors.push('production release must bind an exact source SHA to protected main');
  }
  if (!/release:check -- --published-version/.test(release)) errors.push('production release must enforce the published-version gate');
  if (!/npm run release:check/.test(ci)) errors.push('CI must run the canonical release metadata check');
  if (!/cargo test --manifest-path src-tauri\/Cargo\.toml --example verify_updater_signature/.test(ci)) errors.push('CI must run the updater signature known-vector and tamper test');
  if (!/TAURI_SIGNING_PRIVATE_KEY/.test(release) || !/\.sig/.test(release)) errors.push('Tauri updater signing and signatures must fail closed');
  if (!/\*\.nsis\.zip\.sig/.test(prepareWindows) || /\*\.exe\.sig/.test(prepareWindows)) errors.push('Windows release set must require the single Tauri NSIS updater signature');
  if (!/--example verify_updater_signature/.test(prepareWindows)
    || !/-TauriConfigPath src-tauri\/tauri\.conf\.json/.test(release)
    || !/^minisign-verify\s*=\s*"=0\.2\.5"$/m.test(cargoToml)
    || !/PublicKey::decode/.test(updaterVerifier)
    || !/Signature::decode/.test(updaterVerifier)
    || !/\.verify\(artifact, &signature, false\)/.test(updaterVerifier)) {
    errors.push('Windows staging must cryptographically verify the updater signature with the configured Tauri key');
  }
  if (!appleSecrets.every((name) => new RegExp(`secrets\\.${name}\\b`).test(release))
    || /\bAPPLE_CERTIFICATE\b|\bAPPLE_CERTIFICATE_PASSWORD\b|\bAPP_STORE_CONNECT_API_PRIVATE_KEY\b/.test(release)
    || !/upload-app-store\.sh/.test(release)) {
    errors.push('enabled App Store target must use the canonical signing, profile, team, and API secret contract');
  }
  if (!/^\s*workflow_dispatch:/m.test(testflight)
    || !/source_sha:/.test(testflight)
    || !/^permissions:\r?\n\s+contents:\s+read/m.test(testflight)
    || !/environment:\s*production/.test(testflight)
    || !/test "\$\(git rev-parse HEAD\)" = "\$SOURCE_SHA"/.test(testflight)
    || !appleSecrets.every((name) => new RegExp(`secrets\\.${name}\\b`).test(testflight))
    || !/check-app-store-build\.mjs --require-new-or-existing --allow-next-testflight-build/.test(testflight)
    || !/verify-app-store-readiness\.sh --full/.test(testflight)
    || !/upload-app-store\.sh/.test(testflight)
    || /npx tauri build|release-production|windows-release/i.test(testflight)) {
    errors.push('TestFlight upload must stay an isolated, signed, exact-source macOS-only path');
  }
  const testflightActions = [...testflight.matchAll(/^\s*- uses:\s+([^\s#]+)/gm)].map((match) => match[1]);
  if (testflightActions.some((action) => !/@[0-9a-f]{40}$/.test(action))) errors.push('TestFlight actions must be pinned to immutable commit SHAs');
  const attachAppStoreActions = [...attachAppStore.matchAll(/^\s*- uses:\s+([^\s#]+)/gm)].map((match) => match[1]);
  const attachAppStoreSecrets = [...new Set(
    [...attachAppStore.matchAll(/secrets\.([A-Z0-9_]+)/g)].map((match) => match[1]),
  )].sort();
  const expectedAttachAppStoreSecrets = ['APP_STORE_CONNECT_API_KEY_ID', 'APP_STORE_CONNECT_ISSUER_ID', 'APP_STORE_CONNECT_PRIVATE_KEY'];
  if (!/^\s*workflow_dispatch:/m.test(attachAppStore)
    || !/^permissions:\r?\n\s+contents:\s+read/m.test(attachAppStore)
    || !/environment:\s*production/.test(attachAppStore)
    || !expectedAttachAppStoreSecrets.every((name) => new RegExp(`secrets\\.${name}\\b`).test(attachAppStore))
    || JSON.stringify(attachAppStoreSecrets) !== JSON.stringify(expectedAttachAppStoreSecrets)
    || !/attach-app-store-version-build\.mjs/.test(attachAppStore)
    || attachAppStoreActions.some((action) => !/@[0-9a-f]{40}$/.test(action))) {
    errors.push('App Store version attachment must stay pinned, least-privilege, and production-scoped');
  }
  if (!/if \[ "\$windows" != 'true' \] && \[ "\$mac_app_store" != 'true' \]/.test(release)
    || /Production releases require both Windows and macAppStore targets/.test(release)
    || !/upload_macos_app_store:\r?\n\s+needs: \[preflight, build_macos_app_store, upload_immutable\]/.test(release)
    || !/needs\.preflight\.outputs\.windows != 'true' \|\| needs\.upload_immutable\.result == 'success'/.test(release)
    || !/needs\.preflight\.outputs\.mac_app_store != 'true' \|\| needs\.upload_macos_app_store\.result == 'success'/.test(release)) {
    errors.push('production target graph must support Windows-only, App-Store-only, and combined releases');
  }
  const productionActions = [...release.matchAll(/^\s*- uses:\s+([^\s#]+)/gm)].map((match) => match[1]);
  if (productionActions.some((action) => !/@[0-9a-f]{40}$/.test(action))) errors.push('production actions must be pinned to immutable commit SHAs');
  if (/^ {6}[A-Z0-9_]+:\s*\$\{\{ secrets\./m.test(release)) errors.push('production secrets must be scoped to consuming steps');
  if ((release.match(/apple-actions\/import-codesign-certs@[0-9a-f]{40}/g) ?? []).length !== 4
    || (release.match(/create-keychain:\s*false/g) ?? []).length !== 2
    || (release.match(/install-app-store-profiles\.sh "\$GITHUB_ENV"/g) ?? []).length !== 2
    || /allowProvisioningUpdates|brew install xcodegen|generate-extension-project\.sh/.test(release)
    || !/verify-app-store-readiness\.sh --full/.test(release)
    || !/check-app-store-build\.mjs --require-new-or-existing/.test(release)) {
    errors.push('App Store build and dry-run must fail closed on exact signing, profile, tool, and release-tuple contracts');
  }
  if (!/release-provenance\.json/.test(release) || !/macos-app-store-provenance\.json/.test(release)
    || !/appleArtifactShaAvailable/.test(release)) {
    errors.push('GitHub Release must retain target-independent source and macOS digest provenance');
  }

  const uploadArtifactCount = (release.match(/actions\/upload-artifact@/g) ?? []).length;
  const guardedUploadArtifactCount = (release.match(/^\s{6}- name:[^\n]+\r?\n\s{8}if: inputs\.dry_run == false\r?\n\s{8}uses: actions\/upload-artifact@/gm) ?? []).length;
  if (uploadArtifactCount !== 2 || guardedUploadArtifactCount !== uploadArtifactCount
    || /swatinem\/rust-cache/.test(release)
    || /^\s+cache:\s*npm\s*$/m.test(release)) {
    errors.push('dry-run must not upload retained artifacts or write action caches');
  }

  if ((release.match(/secrets\.DOWNLOADS_SSH_KNOWN_HOSTS/g) ?? []).length !== 2
    || !/StrictHostKeyChecking=yes/.test(publishWindows)
    || !/UserKnownHostsFile=/.test(publishWindows)
    || /StrictHostKeyChecking=accept-new/.test(publishWindows)
    || !/ValidatePattern[^\n]*\r?\n\s*\[string\]\$HostName/.test(publishWindows)
    || !/ValidatePattern[^\n]*\r?\n\s*\[string\]\$User/.test(publishWindows)
    || !/ValidateRange\(1,\s*65535\)/.test(publishWindows)
    || !/RemoteRoot[^\n]*safe|safe[^\n]*RemoteRoot/i.test(publishWindows)
    || !/ConvertTo-PosixSingleQuotedLiteral/.test(publishWindows)) {
    errors.push('production SSH must use pinned host keys and strict safe connection inputs');
  }

  const windowsBuildCount = (release.match(/npx tauri build --bundles nsis/g) ?? []).length;
  if (windowsBuildCount !== 1) errors.push('production release must contain exactly one Windows Tauri build');
  const deployBlocks = [...jobBlocks(release)]
    .filter(([name]) => /deploy|publish|promote|upload/.test(name))
    .map(([, block]) => block)
    .join('\n');
  if (/npx tauri build|cargo build|npm run build/.test(deployBlocks)) errors.push('deploy jobs must not rebuild artifacts');
  if (!/actions\/download-artifact@/.test(deployBlocks)) errors.push('deploy jobs must download retained build artifacts');
  if (!/inputs\.dry_run\s*==\s*false/.test(deployBlocks)) errors.push('all external mutation jobs must be disabled in dry-run mode');
  if (!/different hashes.*hard fail/is.test(release) || !/same hashes.*no-op/is.test(release) || !/latest\.json.*last/is.test(release)) {
    errors.push('exact-byte idempotency and latest.json-last contracts must be explicit');
  }

  if (/\bdevelop\b/.test(runtime) || !/BASE_BRANCH:\s*main/.test(runtime)
    || (runtime.match(/^\s+run: python3 scripts\/update-runtime-versions\.py\s*$/gm) ?? []).length !== 1
    || !/runtime-candidate-\$\{\{ github\.run_id \}\}-\$\{\{ github\.run_attempt \}\}/.test(runtime)
    || !/actions\/upload-artifact@[0-9a-f]{40}/.test(runtime)
    || (runtime.match(/actions\/download-artifact@[0-9a-f]{40}/g) ?? []).length < 3
    || /gh pr merge|--auto/.test(runtime)
    || !/Manual compatibility and trust review required/.test(runtime)) {
    errors.push('runtime updater must resolve one immutable candidate, target main, and require human merge');
  }
  if (/DOWNLOADS_SSH_USER:\s*\$\{\{ vars\.DOWNLOADS_SSH_USER \|\| 'root' \}\}/.test(release)
    || (release.match(/DOWNLOADS_SSH_USER:\s*\$\{\{ vars\.DOWNLOADS_SSH_USER \}\}/g) ?? []).length !== 2) {
    errors.push('production CDN SSH user must be explicit and least-privilege');
  }

  for (const path of [
    'src-tauri/sign-windows-if-configured.ps1',
    'scripts/sign-all-pe.ps1',
    'scripts/verify-signatures.ps1',
    'src-tauri/tauri.microsoftstore.conf.json',
    'scripts/build-store.ps1',
    'scripts/verify-store-installer.ps1',
  ]) {
    if (existsSync(join(root, path))) errors.push(`obsolete release path must be removed: ${path}`);
  }

  if (errors.length) throw new Error(`Workflow checks failed: ${errors.join('; ')}`);
  return expectedWorkflows;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    checkReleaseWorkflows(fileURLToPath(new URL('../../', import.meta.url)));
    console.log(`Release workflow checks passed: ${expectedWorkflows.map((name) => basename(name)).join(', ')}.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
