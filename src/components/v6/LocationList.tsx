import { useMemo } from 'react';
import { Search, Plus } from 'lucide-react';
import type { ServerConfig, Subscription } from '../../stores/app-store';
import { useAppStore } from '../../stores/app-store';
import { buildServerDisplayGroups, serverMatchesGroupQuery } from '../../lib/server-groups';
import ServerRow from './ServerRow';
import SubscriptionStatusBlock from './SubscriptionStatusBlock';

type T = (key: never) => string;

interface Props {
  servers: ServerConfig[];
  activeServer: ServerConfig | null;
  activeSub: Subscription | null;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  onSelect: (server: ServerConfig) => void;
  onAdd: () => void;
  canAdd?: boolean;
  t: T;
}

/** Design "Locations" panel: search, grouped server rows, subscription days block. */
export default function LocationList({
  servers,
  activeServer,
  activeSub,
  searchQuery,
  onSearchChange,
  onSelect,
  onAdd,
  canAdd = true,
  t,
}: Props) {
  const language = useAppStore((state) => state.language);
  const groups = useMemo(() => {
    const filtered = searchQuery.trim()
      ? servers.filter((s) => serverMatchesGroupQuery(s, searchQuery, language))
      : servers;
    return buildServerDisplayGroups(filtered);
  }, [servers, searchQuery, language]);

  const activeId = activeServer?.id;
  const activeSubId = activeServer?.subscriptionId;

  return (
    <div className="v6-location-list flex min-h-0 w-[clamp(280px,32%,392px)] shrink-0 flex-col rounded-[26px] border border-white/[0.09] bg-white/[0.05] p-5">
      {/* Header */}
      <div className="flex items-baseline justify-between px-1 pb-4 pt-0.5">
        <span className="text-[15px] font-semibold text-white">{t('v6Locations' as never)}</span>
        <span className="flex items-center gap-2 text-[12px] text-white/45">
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
              onSelect={onSelect}
            />
          ))
        )}
      </div>

      <SubscriptionStatusBlock
        activeSub={activeSub}
        activeServer={activeServer}
        t={t}
        className="v6-location-sub-status mt-3.5 border-t border-white/[0.08] pt-4"
      />
    </div>
  );
}
