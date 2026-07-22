import { Globe, Monitor, SlidersHorizontal } from 'lucide-react';
import type { ProductMode } from '../../stores/app-store';

type T = (key: never) => string;

interface ModeDef {
  mode: ProductMode;
  icon: typeof Globe;
  badge: string;
  titleKey: string;
  descKey: string;
}

/** Design order: PROXY | TUN | MANUAL. Product default stays 'protected' (TUN). */
const MODES: ModeDef[] = [
  { mode: 'compatibility', icon: Globe, badge: 'PROXY', titleKey: 'v6ModeBrowsersTitle', descKey: 'v6ModeBrowsersDesc' },
  { mode: 'protected', icon: Monitor, badge: 'TUN', titleKey: 'fullDeviceMode', descKey: 'v6ModeProtectedDesc' },
  { mode: 'manual', icon: SlidersHorizontal, badge: 'MANUAL', titleKey: 'v6ModeManualTitle', descKey: 'v6ModeManualDesc' },
];

interface Props {
  current: ProductMode;
  onSelect: (mode: ProductMode) => void;
  disabled?: boolean;
  t: T;
}

/** Row of three design mode cards; selection reconfigures the real transport. */
export default function ModeSelector({ current, onSelect, disabled, t }: Props) {
  return (
    <div className="v6-mode-selector flex gap-3" role="radiogroup" aria-label={t('connectionControls' as never)}>
      {MODES.map((m) => {
        const sel = current === m.mode;
        const Icon = m.icon;
        return (
          <button
            key={m.mode}
            type="button"
            role="radio"
            aria-checked={sel}
            disabled={disabled}
            onClick={() => onSelect(m.mode)}
            className="v6-mode-card flex min-w-0 flex-1 flex-col gap-[9px] rounded-[19px] p-4 text-left transition-[background,border-color] duration-150 disabled:cursor-not-allowed disabled:opacity-50 v6-focus"
            style={{
              background: sel ? 'linear-gradient(150deg, rgba(249,127,22,0.2), rgba(249,127,22,0.06))' : 'rgba(255,255,255,0.04)',
              border: sel ? '1px solid rgba(255,158,56,0.5)' : '1px solid rgba(255,255,255,0.08)',
              boxShadow: sel ? '0 8px 26px rgba(234,109,6,0.2)' : 'none',
            }}
          >
            <div className="v6-mode-card-head flex items-center justify-between gap-1">
              <span
                className="flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-xl"
                style={{
                  color: sel ? '#FFA84E' : 'rgba(255,255,255,0.7)',
                  background: sel ? 'rgba(249,127,22,0.18)' : 'rgba(255,255,255,0.06)',
                  border: sel ? '1px solid rgba(255,158,56,0.35)' : '1px solid rgba(255,255,255,0.08)',
                }}
              >
                <Icon className="h-[19px] w-[19px]" strokeWidth={1.9} />
              </span>
              <span className="v6-mode-card-badges flex min-w-0 items-center gap-1">
                {m.mode === 'protected' && (
                  <span className="truncate rounded-[20px] px-2 py-[3px] text-[9px] font-semibold uppercase tracking-[0.06em]" style={{ background: 'rgba(249,127,22,0.18)', color: '#FFA84E' }}>
                    {t('v6BadgeRecommended' as never)}
                  </span>
                )}
                <span className="rounded-[20px] bg-white/[0.07] px-2 py-[3px] text-[10px] font-semibold tracking-[0.08em] text-white/50">
                  {m.badge}
                </span>
              </span>
            </div>
            <div className="v6-mode-card-title truncate text-[14.5px] font-semibold leading-tight text-white" title={t(m.titleKey as never)}>
              {t(m.titleKey as never)}
            </div>
            <div className="v6-mode-card-description line-clamp-2 text-[11.5px] leading-[1.45] text-white/50" title={t(m.descKey as never)}>
              {t(m.descKey as never)}
            </div>
          </button>
        );
      })}
    </div>
  );
}
