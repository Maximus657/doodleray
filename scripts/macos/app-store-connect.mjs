import { sign } from 'node:crypto';

export const API_ROOT = 'https://api.appstoreconnect.apple.com/v1';

export function createToken(keyId, issuerId, privateKey) {
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

export async function requestJson(url, token, options = {}) {
  const response = await fetch(url, {
    ...options,
    headers: { Authorization: `Bearer ${token}`, ...options.headers },
  });
  if (!response.ok) throw new Error(`App Store Connect API request failed with HTTP ${response.status}.`);
  return response.status === 204 ? undefined : response.json();
}

export async function requestAll(url, token) {
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
