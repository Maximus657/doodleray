import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const updaterEndpoint = 'https://doodleray.clickflare.click/channels/direct/latest.json';
const repositoryRoot = fileURLToPath(new URL('../../', import.meta.url));

function writeJson(root, relativePath, value) {
  const path = join(root, relativePath);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function writeText(root, relativePath, value) {
  const path = join(root, relativePath);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, value);
}

function writeFixture(overrides = {}) {
  const root = mkdtempSync(join(tmpdir(), 'doodleray-release-check-'));
  const version = overrides.version ?? '6.0.2';
  const macBuild = overrides.macBuild ?? 60017;
  const lockVersion = overrides.lockVersion ?? version;
  const xcodeVersion = overrides.xcodeVersion ?? version;
  const pbxVersion = overrides.pbxVersion ?? version;

  writeJson(root, 'release/release.json', {
    version,
    macBuild,
    channel: 'stable',
    targets: overrides.targets ?? { windows: true, macAppStore: true },
  });
  writeJson(root, 'package.json', { version });
  writeJson(root, 'package-lock.json', {
    version: lockVersion,
    packages: { '': { version: lockVersion } },
  });
  writeText(root, 'src-tauri/Cargo.toml', `[package]\nname = "doodleray"\nversion = "${version}"\n`);
  writeJson(root, 'src-tauri/tauri.conf.json', {
    version,
    identifier: 'com.doodlevpn.doodleray',
    bundle: { createUpdaterArtifacts: 'v1Compatible' },
    plugins: { updater: { pubkey: 'test-public-key', endpoints: [updaterEndpoint] } },
  });
  writeJson(root, 'src-tauri/tauri.appstore.conf.json', {
    identifier: 'com.doodleray.doodleray',
    bundle: {
      createUpdaterArtifacts: false,
      resources: {
        'resources/PrivacyInfo.xcprivacy': 'PrivacyInfo.xcprivacy',
        'resources/third-party-notices.txt': 'resources/third-party-notices.txt',
      },
      macOS: { bundleVersion: String(macBuild) },
    },
  });
  writeJson(root, 'src-tauri/tauri.windows.conf.json', {
    bundle: {
      resources: {
        'xray-core/*': 'xray-core/',
        'sing-box*': './',
        'wintun*': './',
        'DoodleRayService.exe': 'DoodleRayService.exe',
      },
    },
  });
  writeText(root, 'src-tauri/build_config.rs', `pub const WINDOWS_RUNTIME_FILES: &[&str] = &[\n    "DoodleRayService.exe",\n    "sing-box.exe",\n    "wintun.dll",\n    "xray-core/xray.exe",\n];\n`);
  writeText(root, 'src-tauri/src/tunnel_service.rs', `pub const TUNNEL_SERVICE_NAME: &str = "DoodleRayTunnelService";\npub const TUNNEL_SERVICE_DISPLAY_NAME: &str = "DoodleRay Tunnel Service";\n`);
  writeText(root, 'src-tauri/src/storage/mod.rs', `const SECURE_STORE_SERVICE: &str = "DoodleRay";\nconst RENDERER_STATE_KEY: &str = "doodleray-storage";\nconst APP_API_SESSION_KEY: &str = "app-api-session-v1";\nconst APP_API_DEVICE_KEY: &str = "app-api-device-v1";\n`);
  writeText(root, 'src-tauri/macos/project.yml', `targets:\n  DoodleRayVPN:\n    entitlements:\n      properties:\n        com.apple.security.application-groups:\n          - group.com.doodleray.doodleray\n    settings:\n      base:\n        PRODUCT_BUNDLE_IDENTIFIER: com.doodleray.doodleray.DoodleRayVPN\n        MARKETING_VERSION: "${xcodeVersion}"\n        CURRENT_PROJECT_VERSION: "${macBuild}"\n`);
  writeText(root, 'src-tauri/macos/DoodleRayAppStoreExtensions.xcodeproj/project.pbxproj', `MARKETING_VERSION = ${pbxVersion};\nCURRENT_PROJECT_VERSION = ${macBuild};\nMARKETING_VERSION = ${pbxVersion};\nCURRENT_PROJECT_VERSION = ${macBuild};\n`);
  writeText(root, 'src-tauri/Entitlements.appstore.plist', '<key>com.apple.security.application-groups</key>\n<array><string>group.com.doodleray.doodleray</string></array>\n');
  writeText(root, 'src-tauri/macos/PacketTunnelProvider/Entitlements.plist', '<key>com.apple.security.application-groups</key>\n<array><string>group.com.doodleray.doodleray</string></array>\n');
  writeText(root, 'src-tauri/macos/HostBridge/NetworkExtensionBridge.m', 'static NSString *const DoodleRayProviderBundleIdentifier = @"com.doodleray.doodleray.DoodleRayVPN";\n[[NSFileManager defaultManager] containerURLForSecurityApplicationGroupIdentifier:@"group.com.doodleray.doodleray"];\n');
  writeText(root, 'src-tauri/nsis-hooks.nsh', '!macro NSIS_HOOK_POSTINSTALL\n!insertmacro DoodleRayRequireFile "DoodleRayService.exe" "DoodleRay Tunnel Service"\n!insertmacro DoodleRayRequireFile "sing-box.exe" "sing-box runtime"\n!insertmacro DoodleRayRequireFile "wintun.dll" "Wintun driver runtime"\n!insertmacro DoodleRayRequireFile "xray-core\\xray.exe" "xray-core runtime"\n!macroend\n');
  return root;
}

async function loadChecker() {
  return import('./check-release.mjs');
}

test('preflight rejects package-lock and Xcode marketing-version drift', async () => {
  const root = writeFixture({ lockVersion: '6.0.1', xcodeVersion: '6.0.0', pbxVersion: '6.0.0' });
  try {
    const { checkRelease } = await loadChecker();
    assert.throws(
      () => checkRelease(root),
      /package-lock\.json root version must equal release\.json version.*XcodeGen MARKETING_VERSION must equal release\.json version.*generated pbxproj MARKETING_VERSION must equal release\.json version/s,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects a candidate equal to the published version', async () => {
  const root = writeFixture();
  try {
    const { checkRelease } = await loadChecker();
    assert.throws(
      () => checkRelease(root, { publishedVersion: '6.0.2' }),
      /release version 6\.0\.2 must be strictly newer than published version 6\.0\.2/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight accepts synchronized metadata and a newer candidate', async () => {
  const root = writeFixture();
  try {
    const { checkRelease } = await loadChecker();
    assert.deepEqual(checkRelease(root, { publishedVersion: '6.0.1' }), {
      version: '6.0.2',
      macBuild: 60017,
      channel: 'stable',
      targets: { windows: true, macAppStore: true },
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight accepts either production target independently', async () => {
  const { checkRelease } = await loadChecker();
  for (const targets of [
    { windows: true, macAppStore: false },
    { windows: false, macAppStore: true },
  ]) {
    const root = writeFixture({ targets });
    try {
      assert.deepEqual(checkRelease(root), {
        version: '6.0.2',
        macBuild: 60017,
        channel: 'stable',
        targets,
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test('preflight rejects a release with no enabled target', async () => {
  const root = writeFixture({ targets: { windows: false, macAppStore: false } });
  try {
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /at least one release target must be enabled/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects an identity contract mismatch', async () => {
  const root = writeFixture();
  try {
    const tauri = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
    tauri.identifier = 'com.example.doodleray';
    writeJson(root, 'src-tauri/tauri.conf.json', tauri);
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /base Tauri identifier must equal com\.doodlevpn\.doodleray/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects a Windows bundle inventory mismatch', async () => {
  const root = writeFixture();
  try {
    const windows = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.windows.conf.json'), 'utf8'));
    delete windows.bundle.resources['wintun*'];
    writeJson(root, 'src-tauri/tauri.windows.conf.json', windows);
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /Windows bundle resources must match the runtime inventory/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects direct engine resources in the App Store bundle', async () => {
  const root = writeFixture();
  try {
    const appStore = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.appstore.conf.json'), 'utf8'));
    appStore.bundle.resources['xray-core/*'] = 'xray-core/';
    writeJson(root, 'src-tauri/tauri.appstore.conf.json', appStore);
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /App Store bundle resources must not include direct engine executables/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects direct engine resources inherited from base and macOS configs', async () => {
  for (const relativePath of ['src-tauri/tauri.conf.json', 'src-tauri/tauri.macos.conf.json']) {
    const root = writeFixture();
    try {
      const effectiveConfig = relativePath === 'src-tauri/tauri.conf.json'
        ? JSON.parse(readFileSync(join(root, relativePath), 'utf8'))
        : { bundle: {} };
      effectiveConfig.bundle.resources = { 'xray-core/*': 'xray-core/' };
      writeJson(root, relativePath, effectiveConfig);
      const { checkRelease } = await loadChecker();
      assert.throws(() => checkRelease(root), /App Store bundle resources must not include direct engine executables/);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test('preflight parses runtime inventory without accepting commented files', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/build_config.rs', 'pub const WINDOWS_RUNTIME_FILES: &[&str] = &[\n  "DoodleRayService.exe",\n  "sing-box.exe",\n  // "wintun.dll",\n  "xray-core/xray.exe",\n];\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /Windows bundle resources must match the runtime inventory/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight ignores commented Rust constants when checking secure-store values', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/src/storage/mod.rs', '// const SECURE_STORE_SERVICE: &str = "DoodleRay";\nconst SECURE_STORE_SERVICE: &str = "WrongStore";\nconst RENDERER_STATE_KEY: &str = "doodleray-storage";\nconst APP_API_SESSION_KEY: &str = "app-api-session-v1";\nconst APP_API_DEVICE_KEY: &str = "app-api-device-v1";\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /secure-store service and keys must retain the compatibility contract/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight reads secure-store compatibility constants from the storage module', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/src/lib.rs', 'mod storage;\n');
    writeText(root, 'src-tauri/src/storage/mod.rs', `const SECURE_STORE_SERVICE: &str = "DoodleRay";\nconst RENDERER_STATE_KEY: &str = "doodleray-storage";\nconst APP_API_SESSION_KEY: &str = "app-api-session-v1";\nconst APP_API_DEVICE_KEY: &str = "app-api-device-v1";\n`);
    const { checkRelease } = await loadChecker();
    assert.doesNotThrow(() => checkRelease(root));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight only reads active NSIS postinstall inventory', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/nsis-hooks.nsh', '!macro NSIS_HOOK_POSTINSTALL\n!insertmacro DoodleRayRequireFile "DoodleRayService.exe" "DoodleRay Tunnel Service"\n!insertmacro DoodleRayRequireFile "sing-box.exe" "sing-box runtime"\n; !insertmacro DoodleRayRequireFile "wintun.dll" "Wintun driver runtime"\n!insertmacro DoodleRayRequireFile "xray-core\\xray.exe" "xray-core runtime"\n!macroend\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /NSIS required-file inventory must match the Windows bundle contract/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight binds the Packet Tunnel identifier to its active project setting', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/macos/project.yml', 'targets:\n  DoodleRayVPN:\n    entitlements:\n      properties:\n        com.apple.security.application-groups:\n          - group.com.doodleray.doodleray\n    settings:\n      base:\n        PRODUCT_BUNDLE_IDENTIFIER: com.example.doodleray\n        MARKETING_VERSION: "6.0.2"\n        CURRENT_PROJECT_VERSION: "60017"\n  Decoy:\n    settings:\n      base:\n        PRODUCT_BUNDLE_IDENTIFIER: com.doodleray.doodleray.DoodleRayVPN\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /Packet Tunnel identifier must equal com\.doodleray\.doodleray\.DoodleRayVPN in project and bridge/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight binds App Group values to entitlement arrays and bridge calls', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/macos/PacketTunnelProvider/Entitlements.plist', '<key>com.apple.security.application-groups</key>\n<array><string>group.example.doodleray</string></array>\n<string>group.com.doodleray.doodleray</string>\n');
    writeText(root, 'src-tauri/macos/HostBridge/NetworkExtensionBridge.m', 'static NSString *const DoodleRayProviderBundleIdentifier = @"com.doodleray.doodleray.DoodleRayVPN";\n// [[NSFileManager defaultManager] containerURLForSecurityApplicationGroupIdentifier:@"group.com.doodleray.doodleray"];\n[[NSFileManager defaultManager] containerURLForSecurityApplicationGroupIdentifier:@"group.example.doodleray"];\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /App Group must equal group\.com\.doodleray\.doodleray in project, host, extension, and bridge/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight ignores XML-commented App Group entitlements', async () => {
  const root = writeFixture();
  try {
    writeText(root, 'src-tauri/macos/PacketTunnelProvider/Entitlements.plist', '<key>com.apple.security.application-groups</key>\n<array><!-- <string>group.com.doodleray.doodleray</string> --><string>group.example.doodleray</string></array>\n');
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /App Group must equal group\.com\.doodleray\.doodleray in project, host, extension, and bridge/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight rejects punctuation drift in exact macOS contract values', async () => {
  const root = writeFixture();
  try {
    const driftedGroup = 'groupXcomYdoodlerayZdoodleray';
    writeText(root, 'src-tauri/macos/project.yml', `targets:\n  DoodleRayVPN:\n    entitlements:\n      properties:\n        com.apple.security.application-groups:\n          - ${driftedGroup}\n    settings:\n      base:\n        PRODUCT_BUNDLE_IDENTIFIER: comXdoodlerayYdoodlerayZDoodleRayVPN\n        MARKETING_VERSION: "6.0.2"\n        CURRENT_PROJECT_VERSION: "60017"\n`);
    writeText(root, 'src-tauri/Entitlements.appstore.plist', `<key>com.apple.security.application-groups</key>\n<array><string>${driftedGroup}</string></array>\n`);
    writeText(root, 'src-tauri/macos/PacketTunnelProvider/Entitlements.plist', `<key>com.apple.security.application-groups</key>\n<array><string>${driftedGroup}</string></array>\n`);
    writeText(root, 'src-tauri/macos/HostBridge/NetworkExtensionBridge.m', `static NSString *const DoodleRayProviderBundleIdentifier = @"comXdoodlerayYdoodlerayZDoodleRayVPN";\n[[NSFileManager defaultManager] containerURLForSecurityApplicationGroupIdentifier:@"${driftedGroup}"];\n`);
    const { checkRelease } = await loadChecker();
    assert.throws(() => checkRelease(root), /Packet Tunnel identifier must equal|App Group must equal/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('preflight compares huge SemVer numeric identifiers without precision loss', async () => {
  const root = writeFixture({ version: '9007199254740993.0.0' });
  try {
    const { checkRelease } = await loadChecker();
    assert.doesNotThrow(() => checkRelease(root, { publishedVersion: '9007199254740992.0.0' }));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('App Store scripts derive release version and build from canonical metadata', () => {
  for (const script of ['build-app-store.sh', 'package-app-store.sh', 'upload-app-store.sh']) {
    const source = readFileSync(join(repositoryRoot, 'scripts/macos', script), 'utf8');
    assert.match(source, /release\/release\.json/);
    assert.match(source, /RELEASE_VERSION/);
    assert.match(source, /RELEASE_BUILD/);
    assert.doesNotMatch(source, /6\.0\.0/);
  }
  assert.match(readFileSync(join(repositoryRoot, 'scripts/macos/build-app-store.sh'), 'utf8'), /MARKETING_VERSION="\$RELEASE_VERSION"/);
});
