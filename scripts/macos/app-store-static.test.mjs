import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const read = (path) => readFileSync(join(root, path), 'utf8');
const readJson = (path) => JSON.parse(read(path));

const HOST_ID = 'com.doodleray.doodleray';
const EXTENSION_ID = 'com.doodleray.doodleray.DoodleRayVPN';
const APP_GROUP = 'group.com.doodleray.doodleray';
const canonicalAppleSecrets = [
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

test('App Store host and Packet Tunnel use the exact IDs and entitlements', () => {
  const config = readJson('src-tauri/tauri.appstore.conf.json');
  const project = read('src-tauri/macos/project.yml');
  const hostEntitlements = read('src-tauri/Entitlements.appstore.plist');
  const extensionEntitlements = read('src-tauri/macos/PacketTunnelProvider/Entitlements.plist');
  const extensionInfo = read('src-tauri/macos/PacketTunnelProvider/Info.plist');

  assert.equal(config.identifier, HOST_ID);
  assert.match(project, new RegExp(`PRODUCT_BUNDLE_IDENTIFIER: ${EXTENSION_ID.replaceAll('.', '\\.')}`));
  assert.match(extensionInfo, /com\.apple\.networkextension\.packet-tunnel/);
  for (const entitlements of [hostEntitlements, extensionEntitlements]) {
    assert.match(entitlements, /<key>com\.apple\.security\.app-sandbox<\/key>\s*<true\/>/);
    assert.match(entitlements, /<key>com\.apple\.security\.network\.client<\/key>\s*<true\/>/);
    assert.match(entitlements, /<key>com\.apple\.developer\.networking\.networkextension<\/key>[\s\S]*?<string>packet-tunnel-provider<\/string>/);
    assert.match(entitlements, new RegExp(`<string>${APP_GROUP.replaceAll('.', '\\.')}<\\/string>`));
  }
});

test('App Store privacy and export declarations are explicit', () => {
  const config = readJson('src-tauri/tauri.appstore.conf.json');
  const info = read('src-tauri/Info.appstore.plist');
  const privacy = read('src-tauri/resources/PrivacyInfo.xcprivacy');

  assert.match(info, /<key>ITSAppUsesNonExemptEncryption<\/key>\s*<false\/>/);
  assert.match(privacy, /<key>NSPrivacyTracking<\/key>\s*<false\/>/);
  for (const dataType of [
    'NSPrivacyCollectedDataTypeUserID',
    'NSPrivacyCollectedDataTypeDeviceID',
    'NSPrivacyCollectedDataTypeProductInteraction',
    'NSPrivacyCollectedDataTypeOtherDiagnosticData',
    'NSPrivacyCollectedDataTypeOtherDataTypes',
  ]) {
    assert.match(privacy, new RegExp(`<string>${dataType}<\\/string>`));
  }
  assert.equal(config.bundle.macOS.infoPlist, 'Info.appstore.plist');
  assert.deepEqual(config.bundle.resources, {
    'resources/PrivacyInfo.xcprivacy': 'PrivacyInfo.xcprivacy',
    'resources/third-party-notices.txt': 'resources/third-party-notices.txt',
  });
});

test('App Store source cannot inherit direct runtimes, dmg, or the updater', () => {
  const config = readJson('src-tauri/tauri.appstore.conf.json');
  const macConfig = readJson('src-tauri/tauri.macos.conf.json');
  const buildScript = read('scripts/macos/build-app-store.sh');
  const rust = read('src-tauri/src/lib.rs');

  assert.equal(existsSync(join(root, 'src-tauri/tauri.direct-macos.conf.json')), false);
  assert.equal(existsSync(join(root, 'src-tauri/sing-box')), false);
  assert.equal(existsSync(join(root, 'src-tauri/src/sysproxy_macos.rs')), false);
  assert.deepEqual(macConfig.bundle.targets, ['app']);
  assert.equal(config.bundle.createUpdaterArtifacts, false);
  assert.deepEqual(config.bundle.targets, ['app']);
  assert.doesNotMatch(JSON.stringify(config.bundle.resources), /xray|sing-box/i);
  assert.match(buildScript, /--bundles app/);
  assert.doesNotMatch(buildScript, /--bundles[^\n]*dmg|allowProvisioningUpdates/);
  assert.match(rust, /#\[cfg\(not\(feature = "app-store"\)\)\][\s\S]{0,120}tauri_plugin_updater/);
});

test('App Store connection accepts only an authorized control-plane profile', () => {
  const rust = read('src-tauri/src/lib.rs');
  const controlPlane = read('src-tauri/src/control_plane/mod.rs');

  assert.match(
    rust,
    /async fn vpn_connect_authorized\(\s*request: ConnectRequest,\s*app: tauri::AppHandle,?\s*\) -> ConnectResult/,
  );
  assert.match(rust, /Direct VPN connection is unavailable in Mac App Store builds/);
  assert.match(controlPlane, /vpn_connect_authorized\(connect_request, app\.clone\(\)\)\.await/);
  assert.doesNotMatch(controlPlane, /vpn_connect\(connect_request, app\.clone\(\)\)\.await/);
});

test('Packet Tunnel rewrites localhost in mixed Xray DNS server arrays', () => {
  const configuration = read('src-tauri/macos/PacketTunnelProvider/PacketTunnelConfiguration.swift');

  assert.match(configuration, /dns\["servers"\] as\? \[Any\]/);
  assert.match(configuration, /server\["address"\] as\? String == "localhost"/);
  assert.match(configuration, /server == "localhost"/);
});

test('Packet Tunnel sends system DNS through Xray DoH', () => {
  const provider = read('src-tauri/macos/PacketTunnelProvider/PacketTunnelProvider.swift');

  assert.match(provider, /NEDNSSettings\(servers:/);
  assert.doesNotMatch(provider, /NEDNSOverHTTPSSettings\(servers:/);
  assert.match(provider, /injectingLocalDNSResolver\(\s*"https:\/\/1\.1\.1\.1\/dns-query"/);
});

test('App Store UI has no external subscription-purchase CTA', () => {
  const subscriptionStatus = read('src/components/v6/SubscriptionStatusBlock.tsx');
  const dashboard = read('src/pages/Dashboard.tsx');

  assert.match(subscriptionStatus, /const canOfferExternalRenewal = !isNetworkExtensionOnlyBuild\(\);/);
  assert.equal((subscriptionStatus.match(/canOfferExternalRenewal &&/g) ?? []).length, 2);
  assert.match(subscriptionStatus, /\{canOfferExternalRenewal && \(\s*<button[\s\S]*?v6RenewCta/);
  assert.match(dashboard, /\{!networkExtensionOnly && \(\s*<button[\s\S]*?v6AppLoginSourceWebLabel/);
});

test('release.json supplies both App Store version fields at build and verification time', () => {
  const release = readJson('release/release.json');
  const packageJson = readJson('package.json');
  const tauri = readJson('src-tauri/tauri.conf.json');
  const appStore = readJson('src-tauri/tauri.appstore.conf.json');
  const cargo = read('src-tauri/Cargo.toml');
  const project = read('src-tauri/macos/project.yml');
  const pbxproj = read('src-tauri/macos/DoodleRayAppStoreExtensions.xcodeproj/project.pbxproj');
  const build = read('scripts/macos/build-app-store.sh');
  const verify = read('scripts/macos/verify-app-store-bundle.sh');

  assert.equal(packageJson.version, release.version);
  assert.equal(tauri.version, release.version);
  assert.match(cargo, new RegExp(`^version = "${release.version.replaceAll('.', '\\.')}"$`, 'm'));
  assert.equal(Object.hasOwn(appStore, 'version'), false);
  assert.equal(Object.hasOwn(appStore.bundle.macOS, 'bundleVersion'), false);
  assert.doesNotMatch(project, /^\s*(?:MARKETING_VERSION|CURRENT_PROJECT_VERSION):/m);
  assert.doesNotMatch(pbxproj, /^\s*(?:MARKETING_VERSION|CURRENT_PROJECT_VERSION) =/m);
  assert.match(build, /release\/release\.json/);
  assert.match(build, /MARKETING_VERSION="\$RELEASE_VERSION"/);
  assert.match(build, /CURRENT_PROJECT_VERSION="\$RELEASE_BUILD"/);
  assert.match(build, /release_config=/);
  assert.match(build, /--config "\$release_config"/);
  assert.match(verify, /release\/release\.json/);
  assert.doesNotMatch(verify, /package\.json|tauri\.appstore\.conf\.json/);
  assert.match(verify, /extension marketing version/);
  assert.match(verify, /extension build number/);
});

test('workflow uses one exact fail-closed Apple secret and profile contract', () => {
  const workflow = read('.github/workflows/release-production.yml');
  const profileInstallerPath = join(root, 'scripts/macos/install-app-store-profiles.sh');
  assert.equal(existsSync(profileInstallerPath), true, 'profile installer is missing');
  if (!existsSync(profileInstallerPath)) return;
  const profileInstaller = read('scripts/macos/install-app-store-profiles.sh');
  const verify = read('scripts/macos/verify-app-store-bundle.sh');
  const upload = read('scripts/macos/upload-app-store.sh');
  const docs = `${read('docs/release-runbook.md')}\n${read('docs/macos-app-store-readiness.md')}`;

  for (const name of canonicalAppleSecrets) {
    assert.match(workflow, new RegExp(`secrets\\.${name}\\b`));
    assert.match(docs, new RegExp(`\\b${name}\\b`));
  }
  for (const legacy of ['APPLE_CERTIFICATE', 'APPLE_CERTIFICATE_PASSWORD', 'APP_STORE_CONNECT_API_PRIVATE_KEY']) {
    assert.doesNotMatch(`${workflow}\n${docs}`, new RegExp(`\\b${legacy}\\b`));
  }
  assert.equal(
    (workflow.match(/apple-actions\/import-codesign-certs@[0-9a-f]{40}/g) ?? []).length,
    4,
  );
  assert.equal((workflow.match(/keychain:\s*signing_temp/g) ?? []).length, 4);
  assert.equal((workflow.match(/create-keychain:\s*false/g) ?? []).length, 2);
  assert.equal((workflow.match(/install-app-store-profiles\.sh "\$GITHUB_ENV"/g) ?? []).length, 2);
  assert.match(workflow, /steps\.import-apple-distribution\.outputs\.keychain-password/);
  assert.doesNotMatch(`${workflow}\n${upload}`, /allowProvisioningUpdates/);
  assert.match(profileInstaller, /TeamIdentifier:0/);
  assert.match(profileInstaller, /Entitlements:com\.apple\.application-identifier/);
  assert.match(profileInstaller, /Entitlements:get-task-allow/);
  assert.match(verify, /distribution profile get-task-allow/);
  assert.match(verify, /SIGNING_DETAILS=/);
  assert.doesNotMatch(verify, /codesign -d --verbose=4 "\$bundle" 2>&1 \| rg/);
  assert.match(verify, /require_array_contains/);
  assert.match(profileInstaller, /Library\/MobileDevice\/Provisioning Profiles/);
  assert.match(upload, /check-app-store-build\.mjs/);
  assert.match(upload, /APP_STORE_TESTFLIGHT_UPLOAD/);
  assert.match(upload, /--allow-next-testflight-build/);
  assert.doesNotMatch(upload, /DoodleRay VPN macOS App Store Host|DoodleRay VPN macOS App Store Extension/);
});

test('production signing never regenerates the privileged extension project', () => {
  const production = read('.github/workflows/release-production.yml');
  const ci = read('.github/workflows/ci.yml');
  const build = read('scripts/macos/build-app-store.sh');

  assert.doesNotMatch(production, /brew install xcodegen|generate-extension-project\.sh/);
  assert.doesNotMatch(build, /generate-extension-project\.sh|\bxcodegen\b/);
  assert.match(ci, /generate-extension-project\.sh/);
  assert.match(ci, /git diff --exit-code -- src-tauri\/macos\/DoodleRayAppStoreExtensions\.xcodeproj/);
});

test('TestFlight upload waits for processing and attaches the internal group', () => {
  const workflow = read('.github/workflows/testflight-macos.yml');

  assert.match(workflow, /scripts\/macos\/upload-app-store\.sh[\s\S]*scripts\/macos\/attach-testflight-build\.mjs/);
  assert.match(workflow, /TESTFLIGHT_INTERNAL_GROUP_NAME: Mac QA/);
});

test('readiness separates portable static proof from macOS release evidence', () => {
  const readiness = read('scripts/macos/verify-app-store-readiness.sh');
  const docs = read('docs/macos-app-store-readiness.md');

  assert.match(readiness, /STATIC PASS/);
  assert.match(readiness, /MACOS RELEASE BLOCKED/);
  assert.match(readiness, /--full/);
  assert.match(readiness, /verify-app-store-bundle\.sh/);
  assert.match(readiness, /for command in node security codesign xcodebuild xcrun plutil lipo/);
  assert.match(docs, /com\.doodlevpn\.doodleray/);
  assert.match(docs, /com\.doodleray\.doodleray/);
  assert.match(docs, /unproven/i);
  assert.equal(existsSync(join(root, 'docs/v6-macos-e2e-audit.md')), false);
});
