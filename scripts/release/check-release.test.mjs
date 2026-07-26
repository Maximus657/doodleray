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
    targets: { windows: true, macAppStore: true },
  });
  writeJson(root, 'package.json', { version });
  writeJson(root, 'package-lock.json', {
    version: lockVersion,
    packages: { '': { version: lockVersion } },
  });
  writeText(root, 'src-tauri/Cargo.toml', `[package]\nname = "doodleray"\nversion = "${version}"\n`);
  writeJson(root, 'src-tauri/tauri.conf.json', {
    version,
    bundle: { createUpdaterArtifacts: 'v1Compatible' },
    plugins: { updater: { pubkey: 'test-public-key', endpoints: [updaterEndpoint] } },
  });
  writeJson(root, 'src-tauri/tauri.appstore.conf.json', {
    bundle: { createUpdaterArtifacts: false, macOS: { bundleVersion: String(macBuild) } },
  });
  writeText(root, 'src-tauri/macos/project.yml', `targets:\n  DoodleRayVPN:\n    settings:\n      base:\n        MARKETING_VERSION: "${xcodeVersion}"\n        CURRENT_PROJECT_VERSION: "${macBuild}"\n`);
  writeText(root, 'src-tauri/macos/DoodleRayAppStoreExtensions.xcodeproj/project.pbxproj', `MARKETING_VERSION = ${pbxVersion};\nCURRENT_PROJECT_VERSION = ${macBuild};\nMARKETING_VERSION = ${pbxVersion};\nCURRENT_PROJECT_VERSION = ${macBuild};\n`);
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
