import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { checkReleaseWorkflows } from './check-workflows.mjs';

const repositoryRoot = fileURLToPath(new URL('../../', import.meta.url));

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
  assert.match(release, /upload_macos_app_store:\r?\n\s+needs: \[preflight, build_macos_app_store, upload_immutable\]/);
  assert.match(release, /needs\.preflight\.outputs\.windows != 'true' \|\| needs\.upload_immutable\.result == 'success'/);
  assert.match(release, /needs\.preflight\.outputs\.mac_app_store != 'true' \|\| needs\.upload_macos_app_store\.result == 'success'/);
});

test('combined release uploads immutable Windows bytes before App Store submission', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.match(release, /upload_macos_app_store:\r?\n\s+needs: \[preflight, build_macos_app_store, upload_immutable\]/);
  assert.match(release, /if: always\(\) && inputs\.dry_run == false && needs\.preflight\.outputs\.mac_app_store == 'true' && needs\.build_macos_app_store\.result == 'success' && \(needs\.preflight\.outputs\.windows != 'true' \|\| needs\.upload_immutable\.result == 'success'\)/);
});

test('macOS-only release can submit without a Windows upload job', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.match(release, /needs\.preflight\.outputs\.windows != 'true' \|\| needs\.upload_immutable\.result == 'success'/);
});

test('production actions are immutable and secrets are scoped to consuming steps', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  const uses = [...release.matchAll(/^\s*- uses:\s+([^\s#]+)(?:\s+#.*)?$/gm)].map((match) => match[1]);
  assert.ok(uses.length > 0);
  for (const action of uses) assert.match(action, /@[0-9a-f]{40}$/);
  assert.doesNotMatch(release, /^ {6}[A-Z0-9_]+:\s*\$\{\{ secrets\./m);
});

test('dry-run validates the complete macOS production contract without publishing', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  for (const name of [
    'MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_BASE64',
    'MAC_INSTALLER_DISTRIBUTION_CERTIFICATE_PASSWORD',
    'APP_STORE_CONNECT_API_KEY_ID',
    'APP_STORE_CONNECT_ISSUER_ID',
    'APP_STORE_CONNECT_PRIVATE_KEY',
  ]) assert.match(release, new RegExp(`secrets\\.${name}\\b`));
  assert.match(release, /verify-app-store-readiness\.sh --full/);
  assert.match(release, /check-app-store-build\.mjs --require-new-or-existing/);
  assert.match(release, /if: inputs\.dry_run == false\r?\n\s+uses: actions\/upload-artifact@/);
  assert.doesNotMatch(release, /if: inputs\.dry_run == true[\s\S]{0,120}(?:gh release|upload-app-store\.sh|Publish-DoodleRayDownloads)/);
});

test('macOS-only release creates an immutable GitHub release with provenance', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.match(release, /publish_github_release:\r?\n\s+needs: \[preflight, upload_immutable, upload_macos_app_store\]/);
  assert.match(release, /needs\.preflight\.outputs\.windows != 'true' \|\| needs\.upload_immutable\.result == 'success'/);
  assert.match(release, /needs\.preflight\.outputs\.mac_app_store != 'true' \|\| needs\.upload_macos_app_store\.result == 'success'/);
  assert.match(release, /release-provenance\.json/);
  assert.match(release, /appleArtifactShaAvailable/);
  assert.match(release, /promote_latest:[\s\S]*needs\.preflight\.outputs\.windows == 'true'/);
  assert.match(release, /Promote latest\.json last/);
});

test('unsigned CI regenerates and checks the tracked extension project', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  const ci = readFileSync(join(repositoryRoot, '.github/workflows/ci.yml'), 'utf8');
  assert.doesNotMatch(release, /brew install xcodegen|generate-extension-project\.sh/);
  assert.match(ci, /generate-extension-project\.sh/);
  assert.match(ci, /git diff --exit-code -- src-tauri\/macos\/DoodleRayAppStoreExtensions\.xcodeproj/);
});

test('runtime update candidates are resolved once and require human merge', () => {
  const runtime = readFileSync(join(repositoryRoot, '.github/workflows/runtime-updates.yml'), 'utf8');
  assert.equal((runtime.match(/^\s+run: python3 scripts\/update-runtime-versions\.py\s*$/gm) ?? []).length, 1);
  assert.match(runtime, /CANDIDATE_NAME:\s*runtime-candidate-\$\{\{ github\.run_id \}\}-\$\{\{ github\.run_attempt \}\}/);
  assert.match(runtime, /actions\/upload-artifact@[0-9a-f]{40}/);
  assert.ok((runtime.match(/actions\/download-artifact@[0-9a-f]{40}/g) ?? []).length >= 3);
  assert.doesNotMatch(runtime, /gh pr merge|--auto/);
  assert.match(runtime, /Manual compatibility and trust review required/);
});

test('CDN deployment requires an explicit least-privilege SSH user', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  assert.doesNotMatch(release, /DOWNLOADS_SSH_USER:\s*\$\{\{ vars\.DOWNLOADS_SSH_USER \|\| 'root' \}\}/);
  assert.equal((release.match(/DOWNLOADS_SSH_USER:\s*\$\{\{ vars\.DOWNLOADS_SSH_USER \}\}/g) ?? []).length, 2);
});

test('legacy Windows QA installers do not depend on the retired repository', () => {
  const qa = readFileSync(join(repositoryRoot, 'scripts/windows-qa/Invoke-DoodleRayUpdatePathQa.ps1'), 'utf8');
  assert.doesNotMatch(qa, /Maximus657\/doodleray\/releases\/download/);
  assert.equal((qa.match(/doodleray\.clickflare\.click\/legacy\/windows\//g) ?? []).length, 2);
});

test('shipping updater remains on the first-party release channel', () => {
  const config = JSON.parse(readFileSync(join(repositoryRoot, 'src-tauri/tauri.conf.json'), 'utf8'));
  assert.deepEqual(config.plugins.updater.endpoints, [
    'https://doodleray.clickflare.click/channels/direct/latest.json',
  ]);
});

test('macOS handoff publishes durable source and digest provenance', () => {
  const release = readFileSync(join(repositoryRoot, '.github/workflows/release-production.yml'), 'utf8');
  const build = readFileSync(join(repositoryRoot, 'scripts/macos/build-app-store.sh'), 'utf8');
  assert.match(build, /APP_BUNDLE=.*DoodleRay\.app/);
  assert.match(release, /--keepParent 'src-tauri\/target\/universal-apple-darwin\/release\/bundle\/macos\/DoodleRay\.app'/);
  assert.match(release, /macos-app-store-provenance\.json/);
  assert.match(release, /DoodleRay-app\.zip[\s\S]{0,240}(?:shasum|sha256)/i);
  assert.match(release, /appleArtifactShaAvailable/);
  assert.match(release, /sourceSha/);
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

test('immutable inventory does not require sha256.txt to hash itself', () => {
  const publisher = readFileSync(join(repositoryRoot, 'scripts/release/Publish-DoodleRayDownloads.ps1'), 'utf8');
  assert.match(publisher, /sha256sum -c sha256\.txt/);
  assert.match(publisher, /tr -d '\\r' < sha256\.txt \| sed -E/);
  assert.match(publisher, /find \. -maxdepth 1 -type f ! -name sha256\.txt -printf '%f\\n' \| sort\)\)/);
  assert.doesNotMatch(publisher, /printf '%s\\n' sha256\.txt/);
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
