import { Loader2 } from 'lucide-react';
import type { ServerConfig } from '../../stores/app-store';
import { countryFlag, protocolLabel } from '../../lib/utils';

function pingColor(p?: number): string {
  if (p === undefined || p <= 0) return 'rgba(255,255,255,0.35)';
  if (p < 80) return '#3ddc84';
  if (p < 160) return '#ffb02e';
  return '#ff6b5a';
}

interface Props {
  server: ServerConfig;
  active: boolean;
  pinging?: boolean;
  onSelect: (server: ServerConfig) => void;
}

/** Design location row: flag, name + protocol line, ping dot + ms. */
export default function ServerRow({ server, active, pinging, onSelect }: Props) {
  const flag = server.countryCode ? countryFlag(server.countryCode) : '🌐';
  const pc = pingColor(server.ping);
  const hasPing = server.ping !== undefined && server.ping > 0;

  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      onClick={() => onSelect(server)}
      className="flex w-full shrink-0 items-center gap-3 rounded-[15px] px-[13px] py-[11px] text-left transition-[background,border-color] duration-150 v6-focus"
      style={{
        background: active ? 'linear-gradient(110deg, rgba(255,107,44,0.22), rgba(255,107,44,0.08))' : 'rgba(255,255,255,0.02)',
        border: active ? '1px solid rgba(255,138,76,0.45)' : '1px solid rgba(255,255,255,0.05)',
        boxShadow: active ? '0 6px 20px rgba(255,90,31,0.18)' : 'none',
      }}
    >
      <span className="w-9 text-center text-[26px] leading-none" style={{ filter: 'drop-shadow(0 2px 4px rgba(0,0,0,0.3))' }}>
        {flag}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14.5px] font-medium text-white">{server.name}</span>
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
