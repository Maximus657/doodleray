import { Loader2, Check } from 'lucide-react';
import type { ServerConfig } from '../../stores/app-store';
import { countryFlag, protocolLabel } from '../../lib/utils';

function pingTone(ping?: number): { color: string; label: string } {
  if (ping === undefined || ping <= 0) return { color: '#6b7488', label: '—' };
  if (ping <= 100) return { color: '#34d399', label: `${ping}` };
  if (ping <= 220) return { color: '#fbbf24', label: `${ping}` };
  return { color: '#f87171', label: `${ping}` };
}

interface Props {
  server: ServerConfig;
  active: boolean;
  pinging?: boolean;
  onSelect: (server: ServerConfig) => void;
}

/** A single selectable server/location in the v6 list. */
export default function ServerRow({ server, active, pinging, onSelect }: Props) {
  const flag = server.countryCode ? countryFlag(server.countryCode) : '🌐';
  const tone = pingTone(server.ping);
  const proto = protocolLabel(server.protocol, server.transport);

  return (
    <button
      type="button"
      role="option"
      aria-selected={active}
      onClick={() => onSelect(server)}
      className={`v6-hover-lift group flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left v6-focus ${
        active ? 'v6-glass' : 'v6-glass-soft'
      }`}
      style={active ? { borderColor: 'rgba(52,211,153,0.4)' } : undefined}
    >
      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-white/[0.06] text-[15px]">{flag}</span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[12.5px] font-medium text-v6-text">{server.name}</span>
        <span className="block truncate text-[10px] uppercase tracking-wider text-v6-muted">{proto}</span>
      </span>
      <span className="flex shrink-0 items-center gap-1.5">
        {pinging ? (
          <Loader2 className="h-3.5 w-3.5 v6-orb-spin text-v6-muted" />
        ) : (
          <span className="text-[11px] font-semibold tabular-nums" style={{ color: tone.color }}>
            {tone.label}
            {tone.label !== '—' && <span className="ml-0.5 text-[8px] font-normal text-v6-muted">ms</span>}
          </span>
        )}
        {active && (
          <span className="grid h-4 w-4 place-items-center rounded-full bg-[#34d399] text-[#0a0d16]">
            <Check className="h-3 w-3" strokeWidth={3} />
          </span>
        )}
      </span>
    </button>
  );
}
