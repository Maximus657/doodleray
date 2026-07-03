import { Power, Loader2, ShieldCheck, ShieldAlert, ShieldX } from 'lucide-react';
import type { OrbState } from './status';
import { ORB_COLORS } from './status';

interface Props {
  state: OrbState;
  primaryLabel: string;
  /** e.g. formatted uptime while connected, or the connection step while connecting */
  subLabel?: string | null;
  serverName?: string | null;
  disabled?: boolean;
  onClick: () => void;
  title?: string;
}

function orbIcon(state: OrbState) {
  const cls = 'h-11 w-11';
  switch (state) {
    case 'connecting':
    case 'disconnecting':
      return <Loader2 className={`${cls} v6-orb-spin`} strokeWidth={2.2} />;
    case 'protected':
      return <ShieldCheck className={cls} strokeWidth={2.2} />;
    case 'degraded':
    case 'limited':
      return <ShieldAlert className={cls} strokeWidth={2.2} />;
    case 'failed':
      return <ShieldX className={cls} strokeWidth={2.2} />;
    default:
      return <Power className={cls} strokeWidth={2.4} />;
  }
}

/**
 * Central connect control. Colour + icon reflect the honest runtime state:
 * idle · connecting · protected · degraded · limited · failed. The parent owns
 * the click semantics (connect / cancel / disconnect) and the label text.
 */
export default function ConnectOrb({
  state,
  primaryLabel,
  subLabel,
  serverName,
  disabled,
  onClick,
  title,
}: Props) {
  const color = ORB_COLORS[state];
  const active = state !== 'idle';
  const busy = state === 'connecting' || state === 'disconnecting';

  return (
    <div className="relative flex flex-col items-center">
      {/* Ambient glow */}
      <div
        aria-hidden
        className="pointer-events-none absolute left-1/2 top-[92px] h-56 w-56 -translate-x-1/2 -translate-y-1/2 rounded-full blur-3xl transition-opacity duration-500"
        style={{ background: color, opacity: active ? 0.28 : 0.12 }}
      />

      <button
        type="button"
        id="connect-button"
        onClick={onClick}
        disabled={disabled}
        title={title || primaryLabel}
        aria-label={title || primaryLabel}
        className="group relative flex h-[184px] w-[184px] flex-col items-center justify-center rounded-full v6-focus transition-transform duration-200 disabled:cursor-not-allowed disabled:opacity-45 hover:scale-[1.015] active:scale-[0.985]"
      >
        {/* Outer ring */}
        <span
          aria-hidden
          className="absolute inset-0 rounded-full"
          style={{
            border: `1.5px solid ${color}55`,
            boxShadow: `0 0 0 1px rgba(255,255,255,0.04), inset 0 0 40px ${color}22`,
          }}
        />
        {/* Pulsing ring while busy / connected */}
        {(busy || state === 'protected' || state === 'degraded' || state === 'limited') && (
          <span
            aria-hidden
            className={`absolute inset-[-6px] rounded-full ${busy ? 'v6-orb-spin' : 'v6-orb-pulse'}`}
            style={{
              border: busy ? `2px solid transparent` : `2px solid ${color}`,
              borderTopColor: busy ? color : undefined,
            }}
          />
        )}
        {/* Glass core */}
        <span className="absolute inset-3 rounded-full v6-glass" style={{ background: `radial-gradient(circle at 50% 30%, ${color}22, rgba(255,255,255,0.02))` }} />

        <span className="relative z-10 flex flex-col items-center gap-2" style={{ color }}>
          {orbIcon(state)}
        </span>
        <span className="relative z-10 mt-2 max-w-[150px] px-3 text-center text-[15px] font-semibold leading-tight tracking-tight text-v6-text [overflow-wrap:anywhere]">
          {primaryLabel}
        </span>
      </button>

      <div className="mt-4 flex min-h-[36px] flex-col items-center gap-1 text-center">
        {serverName && (
          <span className="max-w-[220px] truncate text-[13px] font-medium text-v6-text/90">{serverName}</span>
        )}
        {subLabel && (
          <span
            className="rounded-full border px-2.5 py-0.5 text-[10px] font-medium uppercase tracking-wider"
            style={{ borderColor: `${color}44`, color, background: `${color}12` }}
          >
            {subLabel}
          </span>
        )}
      </div>
    </div>
  );
}
