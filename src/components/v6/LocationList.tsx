import { useMemo } from 'react';
import { Search, Plus, RefreshCw } from 'lucide-react';
import type { ServerConfig, Subscription } from '../../stores/app-store';
import { useAppStore } from '../../stores/app-store';
import { buildServerDisplayGroups, serverMatchesGroupQuery } from '../../lib/server-groups';
import { antiJammerQuotaView, formatQuotaBytes } from '../../lib/ui-format';
import ServerRow from './ServerRow';

type T = (key: never) => string;

/** Design day-count color scale for the subscription block. */
function dayColor(d: number): string {
  if (d <= 0) return '#ff4d4d';
  if (d <= 3) return '#ff5a5f';
  if (d <= 7) return '#F88B24';
  if (d <= 14) return '#ffb02e';
  if (d <= 30) return '#9fd457';
  return '#3ddc84';
}

interface Props {
  servers: ServerConfig[];
  activeServer: ServerConfig | null;
  activeSub: Subscription | null;
  pingingServerIds: Set<string>;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  onSelect: (server: ServerConfig) => void;
  onAdd: () => void;
  canAdd?: boolean;
  onPingAll: () => void;
  t: T;
}

/** Design "Locations" panel: search, grouped server rows, subscription days block. */
export default function LocationList({
  servers,
  activeServer,
  activeSub,
  pingingServerIds,
  searchQuery,
  onSearchChange,
  onSelect,
  onAdd,
  canAdd = true,
  onPingAll,
  t,
}: Props) {
  const groups = useMemo(() => {
    const filtered = searchQuery.trim()
      ? servers.filter((s) => serverMatchesGroupQuery(s, searchQuery))
      : servers;
    return buildServerDisplayGroups(filtered);
  }, [servers, searchQuery]);

  const activeId = activeServer?.id;
  const activeSubId = activeServer?.subscriptionId;
  const language = useAppStore((state) => state.language);

  const expire = activeSub?.traffic?.expire;
  const days = expire && expire > 0 ? Math.max(0, Math.ceil((expire * 1000 - Date.now()) / 86_400_000)) : null;
  const dCol = days !== null ? dayColor(days) : null;
  const quota = activeSub?.antiJammer ? antiJammerQuotaView(activeSub.antiJammer) : null;
  const quotaColor = quota?.tone === 'exhausted' ? '#ff6b5a' : quota?.tone === 'low' ? '#ffb02e' : '#3ddc84';

  return (
    <div className="v6-location-list flex min-h-0 w-[clamp(280px,32%,392px)] shrink-0 flex-col rounded-[26px] border border-white/[0.09] bg-white/[0.05] p-5">
      {/* Header */}
      <div className="flex items-baseline justify-between px-1 pb-4 pt-0.5">
        <span className="text-[15px] font-semibold text-white">{t('v6Locations' as never)}</span>
        <span className="flex items-center gap-2 text-[12px] text-white/45">
          {servers.length} {t('v6ServersCount' as never)}
          <button
            type="button"
            onClick={onPingAll}
            disabled={pingingServerIds.size > 0}
            title={t('v6PingAll' as never)}
            aria-label={t('v6PingAll' as never)}
            className="v6-hover-bright flex h-6 items-center justify-center gap-1.5 rounded-lg border border-white/[0.12] bg-white/[0.07] px-2 text-[11px] font-medium text-white/70 v6-focus disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${pingingServerIds.size > 0 ? 'v6-orb-spin' : ''}`} strokeWidth={2.2} />
            {t('v6PingButton' as never)}
          </button>
          {canAdd && (
            <button
              type="button"
              onClick={onAdd}
              title={t('addSubOrServer' as never)}
              aria-label={t('addSubOrServer' as never)}
              className="v6-hover-bright flex h-6 w-6 items-center justify-center rounded-lg border border-white/[0.12] bg-white/[0.07] text-white/70 v6-focus"
            >
              <Plus className="h-3.5 w-3.5" strokeWidth={2.4} />
            </button>
          )}
        </span>
      </div>

      {/* Search */}
      <div className="v6-glass-inset mb-3.5 flex h-[46px] items-center gap-2.5 rounded-[15px] px-3.5">
        <Search className="h-[17px] w-[17px] shrink-0 text-white/50" strokeWidth={2} />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('v6SearchPlaceholder' as never)}
          aria-label={t('search' as never)}
          className="min-w-0 flex-1 bg-transparent text-[14px] text-white outline-none placeholder:text-white/40"
        />
      </div>

      {/* Rows */}
      <div role="listbox" aria-label={t('servers' as never)} className="-mr-1 flex min-h-0 flex-1 flex-col gap-[7px] overflow-y-auto pr-1">
        {groups.length === 0 ? (
          <div className="grid flex-1 place-items-center px-4 text-center text-[12px] text-white/45">
            {searchQuery.trim() ? t('v6NoResults' as never) : canAdd ? t('addSubOrServer' as never) : t('v6NoResults' as never)}
          </div>
        ) : (
          groups.map((g) => (
            <ServerRow
              key={g.id}
              server={g.selectedServer}
              active={!!activeId && g.servers.some((s) => s.id === activeId && s.subscriptionId === activeSubId)}
              pinging={g.servers.some((s) => pingingServerIds.has(s.id))}
              onSelect={onSelect}
            />
          ))
        )}
      </div>

      {/* Quota and subscription status */}
      {(quota || (days !== null && dCol)) && (
        <div className="mt-3.5 border-t border-white/[0.08] pt-4">
          {quota && (
            <div className={days !== null ? 'mb-4' : ''}>
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
            </div>
          )}

          {days !== null && dCol && (
            <div className={quota ? 'border-t border-white/[0.08] pt-4' : ''}>
              <div className="mb-[11px] flex items-baseline gap-1.5">
                <span className="text-[24px] font-semibold leading-none tabular-nums text-white">{days}</span>
                <span className="text-[12.5px] text-white/45">
                  {days <= 0 ? t('subscriptionExpired' as never) : t('v6DaysLeft' as never)}
                </span>
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
      )}
    </div>
  );
}
