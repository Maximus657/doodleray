import assert from 'node:assert/strict';
import test from 'node:test';
import { selectInternalGroup, selectMacBuild } from './attach-testflight-build.mjs';

test('TestFlight attachment selects only the exact valid macOS build and internal group', () => {
  const builds = {
    data: [{
      id: 'mac-build',
      attributes: { version: '60020', processingState: 'VALID' },
      relationships: { preReleaseVersion: { data: { id: 'mac-version' } } },
    }],
    included: [{ type: 'preReleaseVersions', id: 'mac-version', attributes: { version: '6.0.2', platform: 'MAC_OS' } }],
  };
  const groups = { data: [{ id: 'group-1', attributes: { name: 'DoodleRay 6.0.2 Private QA', isInternalGroup: true } }] };
  assert.equal(selectMacBuild(builds, '6.0.2', '60020').id, 'mac-build');
  assert.equal(selectInternalGroup(groups, 'DoodleRay 6.0.2 Private QA').id, 'group-1');
  assert.equal(selectInternalGroup({ data: [{ id: 'group-name-only', attributes: { name: 'DoodleRay 6.0.2 Private QA' } }] }, 'DoodleRay 6.0.2 Private QA').id, 'group-name-only');
  assert.throws(() => selectMacBuild(builds, '6.0.2', '60021'), /exactly one macOS build/);
  assert.throws(() => selectInternalGroup({ data: [{ id: 'external', attributes: { name: 'DoodleRay 6.0.2 Private QA', isInternalGroup: false } }] }, 'DoodleRay 6.0.2 Private QA'), /exactly one internal TestFlight group/);
});
