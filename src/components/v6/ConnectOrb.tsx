import { ShieldCheck, ShieldAlert, ShieldX } from 'lucide-react';
import type { OrbState } from './status';
import { FlagIcon } from './ServerRow';

interface Props {
  state: OrbState;
  primaryLabel: string;
  /** timer (mm:ss) while connected, connection step while connecting, hint when idle */
  subLabel?: string | null;
  /** honest status text for the pill under the button (Protected/Degraded/Limited/Failed) */
  statusLabel?: string | null;
  serverName?: string | null;
  serverCountryCode?: string | null;
  disabled?: boolean;
  onClick: () => void;
}

/**
 * Design connect core: 172px radial button (gray when off, orange when on),
 * pulse rings + breathing glow, status pill below. Honesty rule: the green
 * "Encrypted" pill appears only for the protected verdict; degraded/limited
 * get an amber pill, failed gets red — never fake-green.
 */
export default function ConnectOrb({
  state,
  primaryLabel,
  subLabel,
  statusLabel,
  serverName,
  serverCountryCode,
  disabled,
  onClick,
}: Props) {
  const off = state === 'idle';
  const busy = state === 'connecting' || state === 'disconnecting';
  const tunnelOn = state === 'protected' || state === 'degraded' || state === 'limited';
  const failed = state === 'failed';

  const btnBackground = failed
    ? 'radial-gradient(120% 120% at 50% 22%, #ff8a7a, #e5484d 62%, #b3261e)'
    : tunnelOn
      ? 'radial-gradient(120% 120% at 50% 22%, #FFA84E, #F97F16 62%, #C55E04)'
      : busy
        ? 'radial-gradient(120% 120% at 50% 22%, #FFC078, #F88B24 62%, #C55E04)'
        : 'radial-gradient(120% 120% at 50% 22%, #84848c, #50505a 60%, #3a3a40)';
  const btnShadow = tunnelOn
    ? '0 8px 26px rgba(234,109,6,0.32), inset 0 1px 0 rgba(255,255,255,0.28)'
    : busy
      ? '0 6px 20px rgba(234,109,6,0.22), inset 0 1px 0 rgba(255,255,255,0.24)'
      : '0 6px 18px rgba(0,0,0,0.22), inset 0 1px 0 rgba(255,255,255,0.14)';

  const pill = tunnelOn || failed
    ? state === 'protected'
      ? { color: '#3ddc84', text: '#9af2c2', bg: 'rgba(61,220,132,0.14)', border: 'rgba(61,220,132,0.3)', Icon: ShieldCheck }
      : failed
        ? { color: '#ff6b5a', text: '#ffb3a8', bg: 'rgba(255,107,90,0.14)', border: 'rgba(255,107,90,0.35)', Icon: ShieldX }
        : { color: '#ffb02e', text: '#ffd28a', bg: 'rgba(255,176,46,0.13)', border: 'rgba(255,176,46,0.32)', Icon: ShieldAlert }
    : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 rounded-[26px] border border-white/[0.08] bg-white/[0.03]">
      <div className="relative flex h-[248px] w-[248px] items-center justify-center">
        {/* Idle: slow white pulse rings */}
        {off && (
          <>
            <span aria-hidden className="v6-pulsering-slow absolute inset-0 rounded-full border border-white/[0.16]" />
            <span aria-hidden className="v6-pulsering-slow absolute inset-0 rounded-full border border-white/[0.11]" style={{ animationDelay: '2.3s' }} />
          </>
        )}
        {/* Active: breathing glow + orange pulse rings */}
        {tunnelOn && (
          <>
            <span
              aria-hidden
              className="v6-breathe pointer-events-none absolute -inset-[22px] rounded-full"
              style={{ background: 'radial-gradient(circle, rgba(249,127,22,0.4), transparent 68%)' }}
            />
            <span aria-hidden className="v6-pulsering absolute inset-0 rounded-full border-[1.5px] border-[#FF9E38]/[0.55]" />
            <span aria-hidden className="v6-pulsering absolute inset-0 rounded-full border-[1.5px] border-[#FF9E38]/[0.45]" style={{ animationDelay: '1.1s' }} />
          </>
        )}

        <button
          type="button"
          id="connect-button"
          onClick={onClick}
          disabled={disabled}
          title={primaryLabel}
          aria-label={primaryLabel}
          className="relative flex h-[172px] w-[172px] flex-col items-center justify-center rounded-full border-none transition-[background,box-shadow] duration-500 active:scale-95 disabled:cursor-not-allowed disabled:opacity-50 v6-focus"
          style={{ background: btnBackground, boxShadow: btnShadow, transitionProperty: 'background, box-shadow, transform' }}
        >
          {busy ? (
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2.6" strokeLinecap="round" className="v6-orb-spin">
              <path d="M12 3a9 9 0 1 0 9 9" />
            </svg>
          ) : (
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="2.5" strokeLinecap="round">
              <line x1="12" y1="3" x2="12" y2="11.5" />
              <path d="M6.8 6.6a8 8 0 1 0 10.4 0" />
            </svg>
          )}
          <span
            className={`mt-2 max-w-[152px] px-1 text-center font-semibold uppercase leading-tight text-white ${
              primaryLabel.length > 12 ? 'text-[13px] tracking-[0.02em]' : primaryLabel.length > 7 ? 'text-[15px] tracking-[0.03em]' : 'text-[19px] tracking-[0.05em]'
            }`}
          >
            {primaryLabel}
          </span>
          {subLabel && <span className="mt-0.5 text-[12.5px] tabular-nums text-white/90">{subLabel}</span>}
        </button>
      </div>

      <div className="flex min-h-[40px] flex-col items-center justify-center gap-2">
        {pill && (
          <div
            className="v6-fadein flex items-center gap-[9px] rounded-[30px] border px-[17px] py-2"
            style={{ background: pill.bg, borderColor: pill.border }}
          >
            <pill.Icon className="h-[15px] w-[15px] shrink-0" style={{ color: pill.color }} strokeWidth={2.4} />
            <span className="flex max-w-[320px] items-center gap-1.5 truncate text-[13px] font-medium" style={{ color: pill.text }}>
              {statusLabel}
              {serverName && (
                <>
                  <span className="opacity-60">·</span>
                  {serverCountryCode && <FlagIcon countryCode={serverCountryCode} size={18} />}
                  <span className="truncate">{serverName}</span>
                </>
              )}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
