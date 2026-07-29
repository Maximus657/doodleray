const FLAG_EMOJI_RE = /[\u{1F1E6}-\u{1F1FF}]{2}/gu;
const PROTOCOL_LABEL_RE = /\b(?:vless|vmess|trojan|shadowsocks|hysteria2?|hy2|tuic|wireguard|reality|grpc|xhttp|websocket|ws|quic)\b/gi;

/**
 * Localized country name for a 2-letter ISO code, in the given UI language.
 * Shared by display (ServerRow) and search matching (server-groups) so a
 * server never shows one name on screen but only matches search under a
 * different (stale, baked-in-at-fetch-time) one.
 */
export function localizedCountryName(countryCode: string | undefined, language: string | undefined): string | undefined {
  if (!language) return undefined;
  const cc = countryCode?.trim().toUpperCase();
  if (!cc || !/^[A-Z]{2}$/.test(cc)) return undefined;
  if (language === 'ru' && cc === 'US') return 'США';
  try {
    return new Intl.DisplayNames([language], { type: 'region' }).of(cc) ?? undefined;
  } catch {
    return undefined;
  }
}

export function displayLocationTitle(name: string, countryCode?: string): string {
  const countryCodeCandidate = countryCode?.trim().toUpperCase();
  const cc = countryCodeCandidate && /^[A-Z]{2}$/.test(countryCodeCandidate)
    ? countryCodeCandidate
    : undefined;
  const cleaned = name
    .replace(FLAG_EMOJI_RE, ' ')
    .trim()
    .replace(PROTOCOL_LABEL_RE, ' ')
    .replace(cc ? new RegExp(`^${cc}(?=[\\s·•|:/—–-])`, 'i') : /$^/, ' ')
    .replace(/[()[\]]/g, ' ')
    .replace(/[\s·•|:/—–-]+/g, ' ')
    .trim();
  return cleaned || cc || 'VPN';
}

export interface AntiJammerQuotaLike {
  limitBytes: number;
  remainingBytes: number;
  lowBalance: boolean;
  exhausted: boolean;
}

export function antiJammerQuotaView(quota: AntiJammerQuotaLike) {
  const limit = Math.max(0, Number.isFinite(quota.limitBytes) ? quota.limitBytes : 0);
  const remaining = Math.min(limit, Math.max(0, Number.isFinite(quota.remainingBytes) ? quota.remainingBytes : 0));
  const ratio = limit > 0 ? remaining / limit : 0;
  const tone = quota.exhausted || remaining === 0 ? 'exhausted' : quota.lowBalance || ratio <= 0.2 ? 'low' : 'normal';
  return { limit, remaining, ratio, tone } as const;
}

export function formatQuotaBytes(bytes: number, locale?: string): string {
  const gb = Math.max(0, bytes) / 1024 ** 3;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(gb)} GB`;
}
