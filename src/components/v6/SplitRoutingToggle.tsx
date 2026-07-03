import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Split, ChevronRight, AlertTriangle } from 'lucide-react';
import { getActiveRoutingRules } from '../../lib/connect-helpers';

type T = (key: never) => string;

/**
 * Compact split-routing summary. Shows how many Workshop rules are active and
 * warns honestly that split routing only applies in protected (TUN) mode.
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
      className="v6-hover-lift v6-glass flex w-full items-center gap-2.5 rounded-xl p-3 text-left v6-focus"
    >
      <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-[#7c6cff1f] text-[#a99bff]">
        <Split className="h-[18px] w-[18px]" strokeWidth={2.1} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[12.5px] font-semibold text-v6-text">{t('workshop' as never)}</span>
        <span className="flex items-center gap-1 truncate text-[10.5px] text-v6-muted">
          {warn ? (
            <><AlertTriangle className="h-3 w-3 shrink-0 text-[#fbbf24]" strokeWidth={2.2} /> {t('splitTunnelingNeedsTun' as never)}</>
          ) : active ? (
            <>{count} {t('v6ActiveRules' as never)}</>
          ) : (
            t('v6SplitRoutingHint' as never)
          )}
        </span>
      </span>
      <ChevronRight className="h-4 w-4 shrink-0 text-v6-muted" strokeWidth={2.2} />
    </button>
  );
}
