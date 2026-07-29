import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { API_ROOT, createToken, requestAll, requestJson } from './app-store-connect.mjs';
import { selectAppId } from './check-app-store-build.mjs';

const APP_BUNDLE_ID = 'com.doodleray.doodleray';

function preReleaseVersions(response) {
  return new Map((response.included ?? [])
    .filter((item) => item?.type === 'preReleaseVersions')
    .map((item) => [item.id, item.attributes]));
}

export function selectMacBuild(response, marketingVersion, buildVersion) {
  const versions = preReleaseVersions(response);
  const matches = (response.data ?? []).filter((build) => {
    const preRelease = versions.get(build?.relationships?.preReleaseVersion?.data?.id);
    return preRelease?.platform === 'MAC_OS'
      && preRelease.version === marketingVersion
      && build?.attributes?.version === buildVersion;
  });
  if (matches.length !== 1) throw new Error(`Expected exactly one macOS build ${marketingVersion} (${buildVersion}).`);
  return matches[0];
}

export function selectInternalGroup(response, name) {
  const matches = (response.data ?? []).filter((group) => group?.attributes?.name === name
    && group?.attributes?.isInternalGroup !== false);
  if (matches.length !== 1) throw new Error(`Expected exactly one internal TestFlight group named ${name}.`);
  return matches[0];
}

function timeoutMs(value) {
  if (!/^\d+$/.test(value ?? '')) throw new Error('TESTFLIGHT_PROCESSING_TIMEOUT_SECONDS must be a positive whole number.');
  const seconds = Number(value);
  if (!Number.isSafeInteger(seconds) || seconds < 1) throw new Error('TESTFLIGHT_PROCESSING_TIMEOUT_SECONDS must be a positive whole number.');
  return seconds * 1000;
}

async function main() {
  const keyId = process.env.APP_STORE_CONNECT_API_KEY_ID;
  const issuerId = process.env.APP_STORE_CONNECT_ISSUER_ID;
  const keyPath = process.env.APP_STORE_CONNECT_API_KEY_PATH;
  if (!keyId || !issuerId || !keyPath) throw new Error('App Store Connect API credentials are missing.');

  const release = JSON.parse(readFileSync(new URL('../../release/release.json', import.meta.url), 'utf8'));
  const groupName = process.env.TESTFLIGHT_INTERNAL_GROUP_NAME ?? `DoodleRay ${release.version} Private QA`;
  const groupId = process.env.TESTFLIGHT_INTERNAL_GROUP_ID;
  if (!groupId) throw new Error('TESTFLIGHT_INTERNAL_GROUP_ID is missing.');
  const token = createToken(keyId, issuerId, readFileSync(keyPath, 'utf8'));
  const appsUrl = new URL(`${API_ROOT}/apps`);
  appsUrl.searchParams.set('filter[bundleId]', APP_BUNDLE_ID);
  appsUrl.searchParams.set('fields[apps]', 'bundleId');
  const appId = selectAppId(await requestJson(appsUrl, token), APP_BUNDLE_ID);

  const groupResponse = await requestJson(`${API_ROOT}/betaGroups/${groupId}`, token);
  const group = selectInternalGroup({ data: [groupResponse.data] }, groupName);

  const buildsUrl = new URL(`${API_ROOT}/builds`);
  buildsUrl.searchParams.set('filter[app]', appId);
  buildsUrl.searchParams.set('include', 'preReleaseVersion');
  buildsUrl.searchParams.set('limit', '200');
  const deadline = Date.now() + timeoutMs(process.env.TESTFLIGHT_PROCESSING_TIMEOUT_SECONDS ?? '1800');
  let build;
  while (true) {
    build = selectMacBuild(await requestAll(buildsUrl, token), release.version, String(release.macBuild));
    if (build.attributes?.processingState === 'VALID') break;
    if (build.attributes?.processingState !== 'PROCESSING') {
      throw new Error(`App Store build ${release.version} (${release.macBuild}) is ${build.attributes?.processingState ?? 'missing'} instead of VALID.`);
    }
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for App Store build ${release.version} (${release.macBuild}) to finish processing.`);
    process.stdout.write(`Waiting for App Store build ${release.version} (${release.macBuild}) to finish processing...\n`);
    await new Promise((resolve) => setTimeout(resolve, 20_000));
  }

  const groupBuilds = await requestAll(`${API_ROOT}/betaGroups/${group.id}/relationships/builds?limit=200`, token);
  if ((groupBuilds.data ?? []).some((item) => item?.id === build.id)) {
    process.stdout.write(`App Store build ${release.version} (${release.macBuild}) is already in ${groupName}.\n`);
    return;
  }
  await requestJson(`${API_ROOT}/betaGroups/${group.id}/relationships/builds`, token, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data: [{ type: 'builds', id: build.id }] }),
  });
  process.stdout.write(`Attached App Store build ${release.version} (${release.macBuild}) to ${groupName}.\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
