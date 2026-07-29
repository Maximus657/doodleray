import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const helperPath = join(root, 'scripts/macos/check-app-store-build.mjs');

test('App Store build lookup accepts only the exact app record', async () => {
  assert.equal(existsSync(helperPath), true, 'App Store build lookup helper is missing');
  if (!existsSync(helperPath)) return;
  const { selectAppId } = await import(pathToFileURL(helperPath));
  assert.equal(selectAppId({ data: [{ id: 'app-1', attributes: { bundleId: 'com.doodleray.doodleray' } }] }, 'com.doodleray.doodleray'), 'app-1');
  assert.throws(() => selectAppId({ data: [] }, 'com.doodleray.doodleray'), /exactly one App Store Connect app/);
  assert.throws(() => selectAppId({ data: [{ id: 'wrong', attributes: { bundleId: 'example.invalid' } }] }, 'com.doodleray.doodleray'), /exactly one App Store Connect app/);
});

test('App Store build lookup no-ops only for the exact usable version tuple', async () => {
  assert.equal(existsSync(helperPath), true, 'App Store build lookup helper is missing');
  if (!existsSync(helperPath)) return;
  const { classifyBuildResponse } = await import(pathToFileURL(helperPath));
  const response = {
    data: [{
      id: 'build-1',
      attributes: { version: '60017', processingState: 'VALID' },
      relationships: { preReleaseVersion: { data: { type: 'preReleaseVersions', id: 'pre-1' } } },
    }],
    included: [{
      type: 'preReleaseVersions',
      id: 'pre-1',
      attributes: { version: '6.0.2', platform: 'MAC_OS' },
    }],
  };
  assert.equal(classifyBuildResponse(response, '6.0.2', '60017'), 'exists');
  assert.equal(classifyBuildResponse(response, '6.0.3', '60017'), 'missing');
  assert.equal(classifyBuildResponse(response, '6.0.2', '60018'), 'missing');
  response.data[0].attributes.processingState = 'PROCESSING';
  assert.equal(classifyBuildResponse(response, '6.0.2', '60017'), 'exists');
  response.data[0].attributes.processingState = 'INVALID';
  assert.throws(() => classifyBuildResponse(response, '6.0.2', '60017'), /cannot be reused/);
});

test('App Store preflight accepts only a newer tuple or an exact usable rerun', async () => {
  assert.equal(existsSync(helperPath), true, 'App Store build lookup helper is missing');
  if (!existsSync(helperPath)) return;
  const { assessRelease } = await import(pathToFileURL(helperPath));
  const existing = {
    data: [
      {
        id: 'build-1',
        attributes: { version: '60016', processingState: 'VALID' },
        relationships: { preReleaseVersion: { data: { type: 'preReleaseVersions', id: 'pre-1' } } },
      },
    ],
    included: [{ type: 'preReleaseVersions', id: 'pre-1', attributes: { version: '6.0.1', platform: 'MAC_OS' } }],
  };
  assert.equal(assessRelease(existing, '6.0.2', '60017'), 'new');
  assert.throws(() => assessRelease(existing, '6.0.1', '60017'), /strictly newer/);
  assert.throws(() => assessRelease(existing, '6.0.2', '60016'), /strictly newer/);

  existing.data[0].attributes.version = '60017';
  existing.included[0].attributes.version = '6.0.2';
  assert.equal(assessRelease(existing, '6.0.2', '60017'), 'exists');
});

test('TestFlight may advance only the build number within the current marketing version', async () => {
  const { assessRelease } = await import(pathToFileURL(helperPath));
  const existing = {
    data: [{
      attributes: { version: '60018', processingState: 'VALID' },
      relationships: { preReleaseVersion: { data: { id: 'pre-current' } } },
    }],
    included: [{
      type: 'preReleaseVersions',
      id: 'pre-current',
      attributes: { version: '6.0.2', platform: 'MAC_OS' },
    }],
  };

  assert.throws(() => assessRelease(existing, '6.0.2', '60019'), /strictly newer/);
  assert.equal(
    assessRelease(existing, '6.0.2', '60019', { allowNextTestFlightBuild: true }),
    'new',
  );
  assert.equal(
    assessRelease(existing, '6.0.2', '60018', { allowNextTestFlightBuild: true }),
    'exists',
  );
});

test('App Store preflight compares large version components without precision loss', async () => {
  const { assessRelease } = await import(pathToFileURL(helperPath));
  const existing = {
    data: [{
      attributes: { version: '9007199254740992', processingState: 'VALID' },
      relationships: { preReleaseVersion: { data: { id: 'pre-large' } } },
    }],
    included: [{
      type: 'preReleaseVersions',
      id: 'pre-large',
      attributes: { version: '9007199254740992.0.0', platform: 'MAC_OS' },
    }],
  };
  assert.equal(
    assessRelease(existing, '9007199254740993.0.0', '9007199254740993'),
    'new',
  );
});
