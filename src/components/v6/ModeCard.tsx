import { MonitorSmartphone, Globe, SlidersHorizontal } from 'lucide-react';
import type { ProductMode } from '../../stores/app-store';

type T = (key: never) => string;

interface ModeDef {
  mode: ProductMode;
  icon: typeof Globe;
  titleKey: string;
  descKey: string;
  badgeKey: string;
  accent: string;
  tone: 'good' | 'warn' | 'neutral';
}

const MODES: ModeDef[] = [
  {
    mode: 'protected',
    icon: MonitorSmartphone,
    titleKey: 'fullDeviceMode',
    descKey: 'v6ModeProtectedDesc',
    badgeKey: 'v6BadgeRecommended',
    accent: '#34d399',
    tone: 'good',
  },
  {
    mode: 'compatibility',
    icon: Globe,
    titleKey: 'v6ModeBrowsersTitle',
    descKey: 'v6ModeBrowsersDesc',
    badgeKey: 'v6BadgeLimited',
    accent: '#f59e0b',
    tone: 'warn',
  },
  {
    mode: 'manual',
    icon: SlidersHorizontal,
    titleKey: 'v6ModeManualTitle',
    descKey: 'v6ModeManualDesc',
    badgeKey: 'v6BadgeAdvanced',
    accent: '#9aa3b4',
    tone: 'neutral',
  },
];

interface Props {
  current: ProductMode;
  onSelect: (mode: ProductMode) => void;
  disabled?: boolean;
  t: T;
}

/** Row of three product-mode cards. Selecting one reconfigures the transport. */
export default function ModeSelector({ current, onSelect, disabled, t }: Props) {
  return (
    <div className="grid grid-cols-3 gap-2.5" role="radiogroup" aria-label={t('connectionControls' as never)}>
      {MODES.map((m) => {
        const active = current === m.mode;
        const Icon = m.icon;
        return (
          <button
            key={m.mode}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            onClick={() => onSelect(m.mode)}
            className={`v6-hover-lift group relative flex flex-col gap-2 rounded-xl p-3 text-left v6-focus disabled:cursor-not-allowed disabled:opacity-50 ${
              active ? 'v6-glass' : 'v6-glass-soft'
            }`}
            style={active ? { borderColor: `${m.accent}66`, boxShadow: `inset 0 0 0 1px ${m.accent}44, 0 8px 24px -16px ${m.accent}` } : undefined}
          >
            <div className="flex items-center justify-between">
              <span
                className="flex h-8 w-8 items-center justify-center rounded-lg"
                style={{ background: `${m.accent}1f`, color: m.accent }}
              >
                <Icon className="h-[18px] w-[18px]" strokeWidth={2.1} />
              </span>
              <span
                className="rounded-full px-1.5 py-0.5 text-[8px] font-semibold uppercase tracking-wider"
                style={{
                  color: m.tone === 'neutral' ? '#9aa3b4' : m.accent,
                  background: m.tone === 'neutral' ? 'rgba(255,255,255,0.06)' : `${m.accent}18`,
                }}
              >
                {t(m.badgeKey as never)}
              </span>
            </div>
            <div className="min-w-0">
              <div className="truncate text-[12.5px] font-semibold text-v6-text">{t(m.titleKey as never)}</div>
              <div className="mt-0.5 line-clamp-2 text-[10.5px] leading-snug text-v6-muted">{t(m.descKey as never)}</div>
            </div>
            {active && (
              <span className="absolute right-2.5 top-2.5 h-1.5 w-1.5 rounded-full" style={{ background: m.accent, boxShadow: `0 0 8px ${m.accent}` }} />
            )}
          </button>
        );
      })}
    </div>
  );
}
