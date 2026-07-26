import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { checkReleaseWorkflows } from './check-workflows.mjs';

const repositoryRoot = new URL('../../', import.meta.url).pathname.replace(/^\/(?:[A-Za-z]:)/, (value) => value.slice(1));

test('repository has one fail-closed production release path without Windows Authenticode', () => {
  assert.doesNotThrow(() => checkReleaseWorkflows(repositoryRoot));
});

test('checker rejects a deploy job that rebuilds Windows', () => {
  const root = mkdtempSync(join(tmpdir(), 'doodleray-workflow-check-'));
  try {
    for (const name of ['ci.yml', 'runtime-updates.yml']) {
      const path = join(root, '.github/workflows', name);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, name === 'runtime-updates.yml' ? 'BASE_BRANCH: main\n' : 'name: ci\n');
    }
    const releasePath = join(root, '.github/workflows/release-production.yml');
    writeFileSync(releasePath, `name: release-production
on:
  workflow_dispatch:
    inputs:
      source_sha:
      dry_run:
concurrency:
  group: release-production
  cancel-in-progress: false
permissions:
  contents: read
jobs:
  preflight:
    steps:
      - run: test "github.ref == 'refs/heads/main'"; git merge-base --is-ancestor "$SOURCE_SHA" origin/main; git rev-parse origin/main; npm run release:check -- --published-version 1.0.0; echo Production releases require both Windows and macAppStore targets
  build-windows:
    steps:
      - run: npx tauri build --bundles nsis
      - run: test -n "$TAURI_SIGNING_PRIVATE_KEY"
      - run: test -f update.sig
  deploy:
    if: inputs.dry_run == false
    steps:
      - uses: actions/download-artifact@v5
      - run: npx tauri build --bundles nsis
      - run: echo APPLE_CERTIFICATE; scripts/macos/upload-app-store.sh; echo different hashes hard fail; echo same hashes no-op; echo latest.json promoted last
`);
    assert.throws(() => checkReleaseWorkflows(root), /deploy jobs must not rebuild artifacts/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
