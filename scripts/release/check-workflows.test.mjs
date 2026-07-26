import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { checkReleaseWorkflows } from './check-workflows.mjs';

const repositoryRoot = new URL('../../', import.meta.url).pathname.replace(/^\/(?:[A-Za-z]:)/, (value) => value.slice(1));

test('repository has one fail-closed production release path without Windows Authenticode', () => {
  assert.doesNotThrow(() => checkReleaseWorkflows(repositoryRoot));
});

test('dry-run builds locally without uploading retained artifacts', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.equal((release.match(/actions\/upload-artifact@/g) ?? []).length, 2);
  assert.equal((release.match(/^\s{6}- name:[^\n]+\r?\n\s{8}if: inputs\.dry_run == false\r?\n\s{8}uses: actions\/upload-artifact@/gm) ?? []).length, 2);
  assert.doesNotMatch(release, /swatinem\/rust-cache/);
  assert.doesNotMatch(release, /^\s+cache:\s+npm\s*$/m);
});

test('production target graph supports Windows-only, macOS-only, and both', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.match(release, /if \[ "\$windows" != 'true' \] && \[ "\$mac_app_store" != 'true' \]/);
  assert.doesNotMatch(release, /Production releases require both Windows and macAppStore targets/);
  assert.match(release, /upload_macos_app_store:\r?\n\s+needs: \[preflight, build_macos_app_store\]/);
  assert.match(release, /needs\.preflight\.outputs\.windows == 'true'[\s\S]*needs\.preflight\.outputs\.mac_app_store != 'true'[\s\S]*needs\.upload_macos_app_store\.result == 'success'/);
});

test('production SSH uses a pinned dedicated known-hosts file and strict inputs', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  const publisher = readFileSync(join(repositoryRoot, 'scripts/release/Publish-DoodleRayDownloads.ps1'), 'utf8');
  assert.equal((release.match(/secrets\.DOWNLOADS_SSH_KNOWN_HOSTS/g) ?? []).length, 2);
  assert.match(publisher, /StrictHostKeyChecking=yes/);
  assert.match(publisher, /UserKnownHostsFile=/);
  assert.doesNotMatch(publisher, /StrictHostKeyChecking=accept-new/);
  assert.match(publisher, /ValidatePattern[^\n]*\r?\n\s*\[string\]\$HostName/);
  assert.match(publisher, /ValidatePattern[^\n]*\r?\n\s*\[string\]\$User/);
  assert.match(publisher, /ValidateRange\(1,\s*65535\)/);
  assert.match(publisher, /RemoteRoot[^\n]*safe|safe[^\n]*RemoteRoot/i);
  assert.match(publisher, /ConvertTo-PosixSingleQuotedLiteral/);
});

test('Windows staging cryptographically verifies the exact updater signature', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  const prepare = readFileSync(join(repositoryRoot, 'scripts/release/Prepare-WindowsRelease.ps1'), 'utf8');
  const cargo = readFileSync(join(repositoryRoot, 'src-tauri/Cargo.toml'), 'utf8');
  const verifier = readFileSync(join(repositoryRoot, 'src-tauri/examples/verify_updater_signature.rs'), 'utf8');
  assert.match(release, /-TauriConfigPath src-tauri\/tauri\.conf\.json/);
  assert.match(prepare, /--example verify_updater_signature/);
  assert.match(prepare, /updaterSignature[\s\S]*Copy-Item/);
  assert.match(cargo, /^minisign-verify\s*=\s*"=0\.2\.5"$/m);
  assert.match(verifier, /PublicKey::decode/);
  assert.match(verifier, /Signature::decode/);
  assert.match(verifier, /\.verify\(artifact, &signature, false\)/);
  assert.match(verifier, /tampered updater bytes must fail verification/);
});

test('actual CI runs the canonical release metadata check', () => {
  const ci = readFileSync(join(repositoryRoot, '.github/workflows/ci.yml'), 'utf8');
  assert.match(ci, /npm run release:check/);
  assert.match(ci, /cargo test --manifest-path src-tauri\/Cargo\.toml --example verify_updater_signature/);
});

test('checker rejects a deploy job that rebuilds Windows', () => {
  const root = mkdtempSync(join(tmpdir(), 'doodleray-workflow-check-'));
  try {
    for (const relativePath of [
      '.github/workflows/ci.yml',
      '.github/workflows/release-production.yml',
      '.github/workflows/runtime-updates.yml',
      'scripts/release/Prepare-WindowsRelease.ps1',
      'scripts/release/Publish-DoodleRayDownloads.ps1',
      'src-tauri/Cargo.toml',
      'src-tauri/examples/verify_updater_signature.rs',
      'src-tauri/tauri.windows.conf.json',
    ]) {
      const destination = join(root, relativePath);
      mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, readFileSync(join(repositoryRoot, relativePath)));
    }
    const releasePath = join(root, '.github/workflows/release-production.yml');
    const release = readFileSync(releasePath, 'utf8');
    writeFileSync(releasePath, release.replace(
      '          (cd windows-release && sha256sum -c sha256.txt)',
      '          npx tauri build --bundles nsis\n          (cd windows-release && sha256sum -c sha256.txt)',
    ));
    assert.throws(() => checkReleaseWorkflows(root), /deploy jobs must not rebuild artifacts/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
