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

// Extract all unique server addresses from a raw xray config
// Used to ping multiple backends for multi-outbound configs (DoodleVPN)
export function getRawConfigAddresses(rawConfig: any): { address: string; port: number }[] {
  if (!rawConfig?.outbounds) return [];
  const seen = new Set<string>();
  const results: { address: string; port: number }[] = [];
  for (const ob of rawConfig.outbounds) {
    // Skip non-proxy outbounds
    if (['direct', 'block', 'dns', 'freedom', 'blackhole'].includes(ob.protocol)) continue;
    if (['direct', 'block', 'dns-out', 'api'].includes(ob.tag)) continue;
    const vnext = ob.settings?.vnext;
    if (vnext) {
      for (const v of vnext) {
        const key = `${v.address}:${v.port}`;
        if (!seen.has(key) && v.address) {
          seen.add(key);
          results.push({ address: v.address, port: v.port || 443 });
        }
      }
    }
    const servers = ob.settings?.servers;
    if (servers) {
      for (const s of servers) {
        const key = `${s.address}:${s.port}`;
        if (!seen.has(key) && s.address) {
          seen.add(key);
          results.push({ address: s.address, port: s.port || 443 });
        }
      }
    }
  }
  return results;
}

// Smart ping: for servers with rawConfig (multi-outbound), try all backend
// addresses and return the best ping. Falls back to primary address for simple servers.
export async function pingServerSmart(
  server: { address: string; port: number; id: string; rawConfig?: any },
  invoke: (cmd: string, args: any) => Promise<any>
): Promise<number> {
  // Collect addresses to try: primary first, then all from rawConfig
  const addresses: { address: string; port: number }[] = [
    { address: server.address, port: server.port },
  ];

  if (server.rawConfig) {
    const extras = getRawConfigAddresses(server.rawConfig);
    for (const e of extras) {
      if (e.address !== server.address || e.port !== server.port) {
        addresses.push(e);
      }
    }
  }

  // Try all addresses, return best ping
  let bestPing = -1;
  for (const addr of addresses) {
    try {
      const result: any = await invoke('ping_server', {
        address: addr.address, port: addr.port, serverId: server.id,
      });
      if (result.ping_ms > 0 && (bestPing < 0 || result.ping_ms < bestPing)) {
        bestPing = result.ping_ms;
        break; // Got a good ping, no need to try more
      }
    } catch {
      // continue to next address
    }
  }
  return bestPing;
}
