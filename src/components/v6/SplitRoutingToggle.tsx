import { ShieldCheck, ChevronRight, AlertTriangle } from 'lucide-react';
import { useWorkshopStore } from '../../stores/workshop-store';

type T = (key: never) => string;

/**
 * Design "split routing" card. Opens the simple v6 rules editor; shows the
 * live enabled-rule count from the Workshop store. Warns honestly when rules
 * exist but the current mode is not TUN (rules only apply in whole-computer).
 */
export default function SplitRoutingToggle({ protectedMode, onOpen, t }: { protectedMode: boolean; onOpen: () => void; t: T }) {
  const count = useWorkshopStore(
    (s) => s.appliedPresets.reduce((n, ap) => n + ap.rules.filter((r) => r.enabled).length, 0)
      + s.myRules.filter((r) => r.enabled).length,
  );

  const active = count > 0;
  const warn = active && !protectedMode;

  return (
    <button
      type="button"
      onClick={onOpen}
      className="v6-hover-bright flex w-full min-w-0 flex-1 items-center gap-[13px] rounded-[20px] border border-white/[0.09] bg-white/[0.05] px-[18px] py-3.5 text-left v6-focus"
    >
      <span className="v6-tile-accent flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-xl">
        <ShieldCheck className="h-[19px] w-[19px]" strokeWidth={1.9} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-medium text-white" title={t('v6SplitTitle' as never)}>{t('v6SplitTitle' as never)}</span>
        <span className="mt-0.5 flex items-center gap-1 truncate text-[11.5px] text-white/45">
          {warn ? (
            <>
              <AlertTriangle className="h-3 w-3 shrink-0 text-[#ffb02e]" strokeWidth={2.2} />
              {t('splitTunnelingNeedsTun' as never)}
            </>
          ) : active ? (
            `${count} ${t('v6ActiveRules' as never)}`
          ) : (
            t('v6SplitSub' as never)
          )}
        </span>
      </span>
      <ChevronRight className="h-4 w-4 shrink-0 text-white/40" strokeWidth={2.2} />
    </button>
  );
}
