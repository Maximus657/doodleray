import { readFileSync } from 'node:fs';
import { sign } from 'node:crypto';
import { pathToFileURL } from 'node:url';

const APP_BUNDLE_ID = 'com.doodleray.doodleray';
const API_ROOT = 'https://api.appstoreconnect.apple.com/v1';

export function selectAppId(response, bundleId) {
  const matches = (response.data ?? []).filter((app) => app?.attributes?.bundleId === bundleId);
  if (matches.length !== 1 || typeof matches[0]?.id !== 'string') {
    throw new Error(`Expected exactly one App Store Connect app for ${bundleId}.`);
  }
  return matches[0].id;
}

function preReleaseVersions(response) {
  return new Map((response.included ?? [])
    .filter((item) => item?.type === 'preReleaseVersions')
    .map((item) => [item.id, item.attributes]));
}

function versionTuple(build, versions) {
  const preReleaseId = build?.relationships?.preReleaseVersion?.data?.id;
  const preRelease = versions.get(preReleaseId);
  if (!preRelease || preRelease.platform !== 'MAC_OS') return null;
  return {
    marketingVersion: preRelease.version,
    buildVersion: build?.attributes?.version,
    processingState: build?.attributes?.processingState,
  };
}

export function classifyBuildResponse(response, marketingVersion, buildVersion) {
  const versions = preReleaseVersions(response);
  const matches = (response.data ?? [])
    .map((build) => versionTuple(build, versions))
    .filter((tuple) => tuple?.marketingVersion === marketingVersion && tuple?.buildVersion === buildVersion);
  if (matches.length === 0) return 'missing';
  if (matches.some((tuple) => tuple.processingState === 'VALID' || tuple.processingState === 'PROCESSING')) return 'exists';
  throw new Error(`App Store build ${marketingVersion} (${buildVersion}) exists in a state that cannot be reused.`);
}

function parseSemver(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(value ?? '');
  if (!match) throw new Error(`Cannot prove App Store version ordering for ${value ?? 'missing version'}.`);
  return match.slice(1).map(BigInt);
}

function compareSemver(left, right) {
  const a = parseSemver(left);
  const b = parseSemver(right);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] > b[index] ? 1 : -1;
  }
  return 0;
}

export function assessRelease(response, marketingVersion, buildVersion) {
  if (classifyBuildResponse(response, marketingVersion, buildVersion) === 'exists') return 'exists';
  if (!/^\d+$/.test(buildVersion)) throw new Error('release.macBuild must be a decimal integer.');

  const versions = preReleaseVersions(response);
  const tuples = (response.data ?? [])
    .map((build) => versionTuple(build, versions))
    .filter(Boolean);
  if (tuples.length === 0) return 'new';
  for (const tuple of tuples) {
    parseSemver(tuple.marketingVersion);
    if (!/^\d+$/.test(tuple.buildVersion ?? '')) {
      throw new Error(`Cannot prove App Store build ordering for ${tuple.buildVersion ?? 'missing build'}.`);
    }
  }

  const latestMarketingVersion = tuples.map((tuple) => tuple.marketingVersion)
    .sort((a, b) => compareSemver(b, a))[0];
  const latestBuildVersion = tuples.map((tuple) => BigInt(tuple.buildVersion))
    .reduce((maximum, value) => value > maximum ? value : maximum);
  if (compareSemver(marketingVersion, latestMarketingVersion) <= 0 || BigInt(buildVersion) <= latestBuildVersion) {
    throw new Error(`App Store release ${marketingVersion} (${buildVersion}) must be strictly newer than the existing macOS release tuple.`);
  }
  return 'new';
}

function createToken(keyId, issuerId, privateKey) {
  const now = Math.floor(Date.now() / 1000);
  const encode = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const unsigned = `${encode({ alg: 'ES256', kid: keyId, typ: 'JWT' })}.${encode({
    iss: issuerId,
    iat: now,
    exp: now + 600,
    aud: 'appstoreconnect-v1',
  })}`;
  const signature = sign('sha256', Buffer.from(unsigned), { key: privateKey, dsaEncoding: 'ieee-p1363' });
  return `${unsigned}.${signature.toString('base64url')}`;
}

async function requestJson(url, token) {
  const response = await fetch(url, { headers: { Authorization: `Bearer ${token}` } });
  if (!response.ok) throw new Error(`App Store Connect API request failed with HTTP ${response.status}.`);
  return response.json();
}

async function requestAll(url, token) {
  const combined = { data: [], included: [] };
  let next = url;
  while (next) {
    const page = await requestJson(next, token);
    combined.data.push(...(page.data ?? []));
    combined.included.push(...(page.included ?? []));
    next = page.links?.next ?? null;
  }
  return combined;
}

async function main() {
  if (!process.argv.includes('--require-new-or-existing')) {
    throw new Error('Expected --require-new-or-existing.');
  }
  const keyId = process.env.APP_STORE_CONNECT_API_KEY_ID;
  const issuerId = process.env.APP_STORE_CONNECT_ISSUER_ID;
  const keyPath = process.env.APP_STORE_CONNECT_API_KEY_PATH;
  if (!keyId) throw new Error('APP_STORE_CONNECT_API_KEY_ID is missing.');
  if (!issuerId) throw new Error('APP_STORE_CONNECT_ISSUER_ID is missing.');
  if (!keyPath) throw new Error('APP_STORE_CONNECT_API_KEY_PATH is missing.');

  const release = JSON.parse(readFileSync(new URL('../../release/release.json', import.meta.url), 'utf8'));
  const token = createToken(keyId, issuerId, readFileSync(keyPath, 'utf8'));
  const appsUrl = new URL(`${API_ROOT}/apps`);
  appsUrl.searchParams.set('filter[bundleId]', APP_BUNDLE_ID);
  appsUrl.searchParams.set('fields[apps]', 'bundleId');
  appsUrl.searchParams.set('limit', '2');
  const appId = selectAppId(await requestJson(appsUrl, token), APP_BUNDLE_ID);

  const buildsUrl = new URL(`${API_ROOT}/builds`);
  buildsUrl.searchParams.set('filter[app]', appId);
  buildsUrl.searchParams.set('include', 'preReleaseVersion');
  buildsUrl.searchParams.set('limit', '200');
  const allBuilds = await requestAll(buildsUrl, token);
  process.stdout.write(assessRelease(allBuilds, release.version, String(release.macBuild)));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
