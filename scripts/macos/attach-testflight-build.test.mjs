import assert from 'node:assert/strict';
import test from 'node:test';
import { selectInternalGroup, selectMacBuild } from './attach-testflight-build.mjs';

test('TestFlight attachment selects the exact macOS build and sole internal group', () => {
  const builds = {
    data: [{
      id: 'mac-build',
      attributes: { version: '60021', processingState: 'VALID' },
      relationships: { preReleaseVersion: { data: { id: 'mac-version' } } },
    }],
    included: [{
      type: 'preReleaseVersions',
      id: 'mac-version',
      attributes: { version: '6.0.2', platform: 'MAC_OS' },
    }],
  };
  const groups = {
    data: [
      { id: 'internal', attributes: { name: 'Internal QA', isInternalGroup: true } },
      { id: 'external', attributes: { name: 'External QA', isInternalGroup: false } },
    ],
  };
  assert.equal(selectMacBuild(builds, '6.0.2', '60021').id, 'mac-build');
  assert.equal(selectMacBuild(builds, '6.0.2', '60022'), undefined);
  assert.equal(selectInternalGroup(groups).id, 'internal');
  groups.data.push({ id: 'second-internal', attributes: { name: 'Staff', isInternalGroup: true } });
  assert.throws(() => selectInternalGroup(groups), /exactly one internal TestFlight group/);
});
