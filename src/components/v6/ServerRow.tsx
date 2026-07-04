import { useState } from 'react';
import { Loader2, Globe } from 'lucide-react';
import type { ServerConfig } from '../../stores/app-store';
import { protocolLabel } from '../../lib/utils';

function pingColor(p?: number): string {
  if (p === undefined || p <= 0) return 'rgba(255,255,255,0.35)';
  if (p < 80) return '#3ddc84';
  if (p < 160) return '#ffb02e';
  return '#ff6b5a';
}

/**
 * Country flag as an image (flagcdn, already allowed by the CSP img-src).
 * Windows has no emoji flags — regional indicators render as bare letters —
 * so images are the only way to match the design. Falls back to a globe.
 */
export function FlagIcon({ countryCode, size = 26 }: { countryCode?: string; size?: number }) {
  const [failed, setFailed] = useState(false);
  const cc = countryCode?.trim().toLowerCase();
  if (!cc || !/^[a-z]{2}$/.test(cc) || failed) {
    return <Globe className="text-white/55" style={{ width: size * 0.77, height: size * 0.77 }} strokeWidth={1.8} />;
  }
  return (
    <img
      src={`https://flagcdn.com/w40/${cc}.png`}
      alt=""
      width={size}
      height={Math.round(size * 0.75)}
      loading="lazy"
      onError={() => setFailed(true)}
      className="rounded-[4px] object-cover"
      style={{ width: size, height: Math.round(size * 0.75), filter: 'drop-shadow(0 2px 4px rgba(0,0,0,0.35))' }}
    />
  );
}

interface Props {
  server: ServerConfig;
  active: boolean;
  pinging?: boolean;
  onSelect: (server: ServerConfig) => void;
}

/**
 * Clean a server name for display next to the flag image: subscription names
 * often embed an emoji flag (🇳🇱) — Windows renders regional indicators as
 * bare letters ("NL") — plus sometimes an ASCII ISO prefix. Strip both.
 */
const FLAG_EMOJI_RE = /[\u{1F1E6}-\u{1F1FF}]{2}/gu;

export function displayServerName(server: Pick<ServerConfig, 'name' | 'countryCode'>): string {
  let name = server.name.replace(FLAG_EMOJI_RE, '').trim();
  const cc = server.countryCode?.trim().toUpperCase();
  if (cc) name = name.replace(new RegExp(`^${cc}[\\s·|-]+`, 'i'), '').trim();
  return name || server.name;
}

/** Design location row: flag, name + protocol line, ping dot + ms. */
export default function ServerRow({ server, active, pinging, onSelect }: Props) {
  const pc = pingColor(server.ping);
  const hasPing = server.ping !== undefined && server.ping > 0;
  const name = displayServerName(server);

  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      onClick={() => onSelect(server)}
      className="flex w-full shrink-0 items-center gap-3 rounded-[15px] px-[13px] py-[11px] text-left transition-[background,border-color] duration-150 v6-focus"
      style={{
        background: active ? 'linear-gradient(110deg, rgba(249,127,22,0.22), rgba(249,127,22,0.08))' : 'rgba(255,255,255,0.02)',
        border: active ? '1px solid rgba(255,158,56,0.45)' : '1px solid rgba(255,255,255,0.05)',
        boxShadow: active ? '0 6px 20px rgba(234,109,6,0.18)' : 'none',
      }}
    >
      <span className="flex w-9 shrink-0 items-center justify-center">
        <FlagIcon countryCode={server.countryCode} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14.5px] font-medium text-white" title={server.name}>{name}</span>
        <span className="mt-0.5 block truncate text-[12px] text-white/45">{protocolLabel(server.protocol, server.transport)}</span>
      </span>
      <span className="flex items-center gap-[7px]">
        {pinging ? (
          <Loader2 className="h-3.5 w-3.5 v6-orb-spin text-white/50" />
        ) : (
          <>
            <span className="h-[7px] w-[7px] rounded-full" style={{ background: pc, boxShadow: hasPing ? `0 0 8px ${pc}` : 'none' }} />
            <span className="w-[46px] text-right text-[12.5px] tabular-nums text-white/60">
              {hasPing ? `${server.ping} ms` : '—'}
            </span>
          </>
        )}
      </span>
    </button>
  );
}
