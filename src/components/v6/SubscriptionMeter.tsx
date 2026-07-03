import { RefreshCw, Gauge, AlertTriangle } from 'lucide-react';
import type { Subscription } from '../../stores/app-store';
import { getSubscriptionTrafficStatus } from '../../lib/subscription-status';
import { formatBytes } from '../../lib/utils';

type T = (key: never) => string;

interface Props {
  subscription: Subscription | null;
  refreshing?: boolean;
  onRefresh?: () => void;
  t: T;
}

/** Compact quota + expiry meter for the active subscription. Honest about limits. */
export default function SubscriptionMeter({ subscription, refreshing, onRefresh, t }: Props) {
  if (!subscription) return null;
  const st = getSubscriptionTrafficStatus(subscription);
  const expireDate =
    subscription.traffic?.expire && subscription.traffic.expire > 0
      ? new Date(subscription.traffic.expire * 1000)
      : null;

  const barColor = st.isLimited ? '#f87171' : st.usedPercent > 85 ? '#fbbf24' : '#7c6cff';

  return (
    <div className="v6-glass rounded-xl p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          <Gauge className="h-3.5 w-3.5 shrink-0 text-v6-muted" strokeWidth={2.2} />
          <span className="truncate text-[12px] font-semibold text-v6-text">{subscription.name}</span>
        </div>
        {onRefresh && (
          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            title={t('retry' as never)}
            aria-label={t('retry' as never)}
            className="grid h-6 w-6 place-items-center rounded-md text-v6-muted hover:bg-white/10 hover:text-v6-text v6-focus disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'v6-orb-spin' : ''}`} strokeWidth={2.2} />
          </button>
        )}
      </div>

      {st.hasQuota ? (
        <>
          <div className="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-white/[0.07]">
            <div
              className="h-full rounded-full transition-[width] duration-500"
              style={{ width: `${Math.min(100, st.usedPercent)}%`, background: barColor }}
            />
          </div>
          <div className="mt-1.5 flex items-center justify-between text-[10.5px] text-v6-muted">
            <span className="tabular-nums">
              {formatBytes(st.used)} / {formatBytes(st.total)}
            </span>
            {expireDate && <span className="tabular-nums">{t('validUntil' as never)} {expireDate.toLocaleDateString()}</span>}
          </div>
        </>
      ) : (
        <div className="mt-2 text-[10.5px] text-v6-muted">{t('trafficUnavailable' as never)}</div>
      )}

      {st.isLimited && (
        <div className="mt-2 flex items-center gap-1.5 rounded-lg bg-[#f8717118] px-2 py-1.5 text-[10.5px] font-medium text-[#fca5a5]">
          <AlertTriangle className="h-3.5 w-3.5 shrink-0" strokeWidth={2.2} />
          {t((st.reason === 'expired' ? 'subscriptionExpired' : 'subscriptionLimited') as never)}
        </div>
      )}
    </div>
  );
}
