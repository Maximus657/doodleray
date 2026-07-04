import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ShieldCheck, ChevronRight, AlertTriangle } from 'lucide-react';
import { getActiveRoutingRules } from '../../lib/connect-helpers';

type T = (key: never) => string;

/**
 * Design "Local & popular sites direct" card, wired honestly: split routing is
 * driven by real Workshop rules (no fake switch — the card opens Workshop and
 * shows the live rule count). Warns when rules exist but the mode is not TUN.
 */
export default function SplitRoutingToggle({ protectedMode, t }: { protectedMode: boolean; t: T }) {
  const navigate = useNavigate();
  const [count, setCount] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    getActiveRoutingRules()
      .then((rules) => { if (!cancelled) setCount(rules.length); })
      .catch(() => { if (!cancelled) setCount(0); });
    return () => { cancelled = true; };
  }, []);

  const active = (count ?? 0) > 0;
  const warn = active && !protectedMode;

  return (
    <button
      type="button"
      onClick={() => navigate('/workshop')}
      className="v6-hover-bright flex min-w-0 flex-1 items-center gap-[13px] rounded-[20px] border border-white/[0.09] bg-white/[0.05] px-[18px] py-3.5 text-left v6-focus"
    >
      <span className="v6-tile-accent flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-xl">
        <ShieldCheck className="h-[19px] w-[19px]" strokeWidth={1.9} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-medium text-white">{t('v6SplitTitle' as never)}</span>
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
