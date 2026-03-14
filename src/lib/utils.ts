// Country code to flag emoji
export function countryFlag(code: string): string {
  if (!code || code.length !== 2) return '🌐';
  const codePoints = code
    .toUpperCase()
    .split('')
    .map((c) => 0x1f1e6 + c.charCodeAt(0) - 65);
  return String.fromCodePoint(...codePoints);
}

// Format bytes to human-readable
export function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond < 1024) return `${bytesPerSecond.toFixed(0)} B/s`;
  if (bytesPerSecond < 1024 * 1024)
    return `${(bytesPerSecond / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSecond / (1024 * 1024)).toFixed(2)} MB/s`;
}

// Format ping with color class
export function pingColor(ping: number | undefined): string {
  if (ping === undefined) return 'text-text-muted';
  if (ping < 100) return 'text-success';
  if (ping < 300) return 'text-warning';
  return 'text-danger';
}

// Format ping display
export function formatPing(ping: number | undefined): string {
  if (ping === undefined) return '— ms';
  if (ping < 0) return 'timeout';
  return `${ping} ms`;
}

// Short protocol label
export function protocolLabel(protocol: string, transport: string): string {
  // QUIC-based protocols don't need transport suffix
  if (protocol === 'hysteria2') return 'HY2 • QUIC';
  if (protocol === 'tuic') return 'TUIC • QUIC';
  if (protocol === 'wireguard') return 'WG • UDP';
  const transportSuffix = transport !== 'tcp' ? `+${transport}` : '';
  return `${protocol.toUpperCase()}${transportSuffix}`;
}

// Generate unique color from string (for server groups)
export function stringToColor(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  return `hsl(${hue}, 65%, 55%)`;
}

// Time format for speed graph
export function formatTime(date: Date): string {
  return date.toLocaleTimeString('en', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}
