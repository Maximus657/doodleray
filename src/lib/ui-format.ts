const FLAG_EMOJI_RE = /[\u{1F1E6}-\u{1F1FF}]{2}/gu;
const PROTOCOL_LABEL_RE = /\b(?:vless|vmess|trojan|shadowsocks|hysteria2?|hy2|tuic|wireguard|reality|grpc|xhttp|websocket|ws|quic)\b/gi;

export function displayLocationTitle(name: string, countryCode?: string): string {
  const cc = countryCode?.trim().toUpperCase();
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
