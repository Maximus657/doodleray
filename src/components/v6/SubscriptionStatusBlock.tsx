import type { ServerConfig, Subscription } from '../../stores/app-store';
import { antiJammerQuotaView, formatQuotaBytes } from '../../lib/ui-format';
import { isAntiJammerOrReserveServer } from '../../lib/app-control-plane';
import { useAppStore } from '../../stores/app-store';

type T = (key: never) => string;

const ACCOUNT_URL = 'https://doodlevpn.online/account';
const LOW_QUOTA_RENEW_BYTES = 1024 ** 3; // Show the renew CTA once under 1GB remains.

async function openAccountPage() {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(ACCOUNT_URL);
  } catch {
    window.open(ACCOUNT_URL, '_blank');
  }
}

/** Design day-count color scale for the subscription block. */
function dayColor(d: number): string {
  if (d <= 3) return '#ff5a5f';
  if (d <= 7) return '#ffb02e';
  return '#3ddc84';
}

interface Props {
  activeSub: Subscription | null;
  activeServer: ServerConfig | null;
  t: T;
  className?: string;
}

/**
 * Anti-jammer quota bar (only for the selected "Обход БС"/"Резерв" location)
 * and days-left bar. Rendered once in LocationList (wide layout) and once
 * under the connect button (compact layout) — CSS picks which stays visible.
 */
export default function SubscriptionStatusBlock({ activeSub, activeServer, t, className }: Props) {
  const language = useAppStore((state) => state.language);
  const expire = activeSub?.traffic?.expire;
  const days = expire && expire > 0 ? Math.max(0, Math.ceil((expire * 1000 - Date.now()) / 86_400_000)) : null;
  const dCol = days !== null ? dayColor(days) : null;
  const quota = activeSub?.antiJammer && isAntiJammerOrReserveServer(activeServer)
    ? antiJammerQuotaView(activeSub.antiJammer)
    : null;
  const quotaColor = quota?.tone === 'exhausted' ? '#ff6b5a' : quota?.tone === 'low' ? '#ffb02e' : '#3ddc84';

  if (!quota && !(days !== null && dCol)) return null;

  return (
    <div className={className}>
      {quota && (
        <div className={`v6-fadein ${days !== null ? 'mb-4' : ''}`}>
          <div className="mb-2 flex items-baseline justify-between gap-3">
            <span className="text-[12.5px] font-semibold text-white">{t('v6AntiJammer' as never)}</span>
            <span className="text-right text-[11.5px] tabular-nums text-white/65">
              {formatQuotaBytes(quota.remaining, language)} {t('v6QuotaOf' as never)} {formatQuotaBytes(quota.limit, language)}
            </span>
          </div>
          <div className="h-[7px] overflow-hidden rounded-md bg-white/10">
            <div
              className="h-full rounded-md transition-[width] duration-500"
              style={{ width: `${(quota.ratio * 100).toFixed(1)}%`, background: quotaColor, boxShadow: `0 0 10px ${quotaColor}66` }}
            />
          </div>
          <p className="mt-2 text-[11px] leading-snug text-white/45">{t('v6RegularTrafficUnlimited' as never)}</p>
          {quota.tone !== 'normal' && (
            <p className={`mt-1 text-[11px] leading-snug ${quota.tone === 'exhausted' ? 'text-[#ff9b91]' : 'text-[#ffd28a]'}`}>
              {t((quota.tone === 'exhausted' ? 'v6AntiJammerExhausted' : 'v6AntiJammerLow') as never)}
            </p>
          )}
          {quota.remaining <= LOW_QUOTA_RENEW_BYTES && (
            <button type="button" onClick={openAccountPage} className="v6-hover-bright mt-2 w-full rounded-[11px] border border-white/[0.12] bg-white/[0.06] py-1.5 text-[11.5px] font-semibold text-[#FFA84E] v6-focus">
              {t('v6RenewCta' as never)}
            </button>
          )}
        </div>
      )}

      {days !== null && dCol && (
        <div className={quota ? 'border-t border-white/[0.08] pt-4' : ''}>
          <div className="mb-[11px] flex items-baseline justify-between gap-1.5">
            <span className="flex items-baseline gap-1.5">
              <span className="text-[24px] font-semibold leading-none tabular-nums text-white">{days}</span>
              <span className="text-[12.5px] text-white/45">
                {days <= 0 ? t('subscriptionExpired' as never) : t('v6DaysLeft' as never)}
              </span>
            </span>
            <button type="button" onClick={openAccountPage} className="v6-hover-bright shrink-0 rounded-[11px] border border-white/[0.12] bg-white/[0.06] px-2.5 py-1 text-[11.5px] font-semibold text-[#FFA84E] v6-focus">
              {t('v6RenewCta' as never)}
            </button>
          </div>
          <div className="h-[7px] overflow-hidden rounded-md bg-white/10">
            <div
              className="h-full rounded-md transition-[width] duration-500"
              style={{ width: `${(Math.min(1, days / 30) * 100).toFixed(1)}%`, background: dCol, boxShadow: `0 0 10px ${dCol}66` }}
            />
          </div>
        </div>
      )}
    </div>
  );
}
