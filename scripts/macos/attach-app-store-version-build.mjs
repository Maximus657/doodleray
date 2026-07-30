import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import {
  API_ROOT,
  createToken,
  requestAll,
  requestJson,
  selectAppId,
} from './check-app-store-build.mjs';
import { selectMacBuild } from './attach-testflight-build.mjs';

const APP_BUNDLE_ID = 'com.doodleray.doodleray';

export function selectEditableMacVersion(response, marketingVersion) {
  const matches = (response.data ?? []).filter((version) => (
    version?.attributes?.platform === 'MAC_OS'
    && version.attributes?.versionString === marketingVersion
    && version.attributes?.appStoreState === 'PREPARE_FOR_SUBMISSION'
  ));
  if (matches.length !== 1) {
    throw new Error(`Expected exactly one editable macOS App Store version ${marketingVersion}.`);
  }
  return matches[0];
}

async function main() {
  const keyId = process.env.APP_STORE_CONNECT_API_KEY_ID;
  const issuerId = process.env.APP_STORE_CONNECT_ISSUER_ID;
  const keyPath = process.env.APP_STORE_CONNECT_API_KEY_PATH;
  if (!keyId || !issuerId || !keyPath) throw new Error('App Store Connect API credentials are missing.');

  const release = JSON.parse(readFileSync(new URL('../../release/release.json', import.meta.url), 'utf8'));
  const token = createToken(keyId, issuerId, readFileSync(keyPath, 'utf8'));
  const appsUrl = new URL(`${API_ROOT}/apps`);
  appsUrl.searchParams.set('filter[bundleId]', APP_BUNDLE_ID);
  appsUrl.searchParams.set('fields[apps]', 'bundleId');
  const appId = selectAppId(await requestJson(appsUrl, token), APP_BUNDLE_ID);

  const versionsUrl = new URL(`${API_ROOT}/apps/${appId}/appStoreVersions`);
  versionsUrl.searchParams.set('filter[platform]', 'MAC_OS');
  versionsUrl.searchParams.set('fields[appStoreVersions]', 'versionString,platform,appStoreState');
  versionsUrl.searchParams.set('limit', '200');
  const version = selectEditableMacVersion(await requestAll(versionsUrl, token), release.version);

  const buildsUrl = new URL(`${API_ROOT}/builds`);
  buildsUrl.searchParams.set('filter[app]', appId);
  buildsUrl.searchParams.set('include', 'preReleaseVersion');
  buildsUrl.searchParams.set('limit', '200');
  const build = selectMacBuild(await requestAll(buildsUrl, token), release.version, String(release.macBuild));
  if (build?.attributes?.processingState !== 'VALID') {
    throw new Error(`App Store build ${release.version} (${release.macBuild}) is not VALID.`);
  }

  const relationshipUrl = `${API_ROOT}/appStoreVersions/${version.id}/relationships/build`;
  const current = await requestJson(relationshipUrl, token);
  if (current?.data?.id === build.id) {
    process.stdout.write(`App Store version ${release.version} already uses build ${release.macBuild}.\n`);
    return;
  }
  await requestJson(relationshipUrl, token, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data: { type: 'builds', id: build.id } }),
  });
  process.stdout.write(`Attached build ${release.macBuild} to macOS App Store version ${release.version}.\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
