const URL_TRAILING_PUNCTUATION = /[)\].,;!?]+$/;

function splitTrailingPunctuation(value: string) {
  const match = value.match(URL_TRAILING_PUNCTUATION);
  if (!match) return { clean: value, trailing: '' };
  return {
    clean: value.slice(0, -match[0].length),
    trailing: match[0],
  };
}

export function sanitizeSensitiveText(value?: string | null): string | null {
  if (!value) return null;

  return value
    .replace(/\b(vless|vmess|trojan|ss|hy2|tuic|wg):\/\/[^\s"'<>]+/gi, '$1://[redacted]')
    .replace(/https?:\/\/[^\s"'<>]+/gi, (match) => {
      const { clean, trailing } = splitTrailingPunctuation(match);
      try {
        const url = new URL(clean);
        const path = url.pathname && url.pathname !== '/' ? '/...' : '';
        const query = url.search ? '?...' : '';
        return `${url.protocol}//${url.host}${path}${query}${trailing}`;
      } catch {
        return `https://[redacted]${trailing}`;
      }
    })
    .replace(/\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b/gi, '[uuid]')
    .replace(/("?(?:password|uuid|id|private_key|publicKey|shortId|token|key)"?\s*[:=]\s*)["']?[^"',\s}]+/gi, '$1[redacted]')
    .slice(0, 4000);
}

export function sanitizeLogMessage(message: string): string {
  return sanitizeSensitiveText(message) || '';
}

export function describeSubscriptionSource(rawUrl: string): string {
  try {
    const url = new URL(rawUrl);
    return `subscription link (${url.host})`;
  } catch {
    return 'subscription link';
  }
}
