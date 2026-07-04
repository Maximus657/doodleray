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
    <div className="flex gap-3" role="radiogroup" aria-label={t('connectionControls' as never)}>
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
            className="flex flex-1 flex-col gap-[9px] rounded-[19px] p-4 text-left transition-[background,border-color] duration-150 disabled:cursor-not-allowed disabled:opacity-50 v6-focus"
            style={{
              background: sel ? 'linear-gradient(150deg, rgba(255,107,44,0.2), rgba(255,107,44,0.06))' : 'rgba(255,255,255,0.04)',
              border: sel ? '1px solid rgba(255,138,76,0.5)' : '1px solid rgba(255,255,255,0.08)',
              boxShadow: sel ? '0 8px 26px rgba(255,90,31,0.2)' : 'none',
            }}
          >
            <div className="flex items-center justify-between">
              <span
                className="flex h-[38px] w-[38px] items-center justify-center rounded-xl"
                style={{
                  color: sel ? '#FF9A56' : 'rgba(255,255,255,0.7)',
                  background: sel ? 'rgba(255,107,44,0.18)' : 'rgba(255,255,255,0.06)',
                  border: sel ? '1px solid rgba(255,138,76,0.35)' : '1px solid rgba(255,255,255,0.08)',
                }}
              >
                <Icon className="h-[19px] w-[19px]" strokeWidth={1.9} />
              </span>
              <span className="rounded-[20px] bg-white/[0.07] px-2 py-[3px] text-[10px] font-semibold tracking-[0.08em] text-white/50">
                {m.badge}
              </span>
            </div>
            <div className="text-[14.5px] font-semibold leading-tight text-white">
              {t(m.titleKey as never)}
              {m.mode === 'protected' && (
                <span className="ml-1.5 align-middle text-[9px] font-semibold uppercase tracking-wider text-[#FF9A56]">
                  {t('v6BadgeRecommended' as never)}
                </span>
              )}
            </div>
            <div className="line-clamp-2 text-[11.5px] leading-[1.45] text-white/50">{t(m.descKey as never)}</div>
          </button>
        );
      })}
    </div>
  );
}
