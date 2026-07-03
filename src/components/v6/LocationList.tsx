import { useMemo } from 'react';
import { Search, MapPin } from 'lucide-react';
import type { ServerConfig } from '../../stores/app-store';
import { buildServerDisplayGroups, serverMatchesGroupQuery } from '../../lib/server-groups';
import ServerRow from './ServerRow';

type T = (key: never) => string;

interface Props {
  servers: ServerConfig[];
  activeServer: ServerConfig | null;
  pingingServerIds: Set<string>;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  onSelect: (server: ServerConfig) => void;
  t: T;
}

/** Searchable list of location groups (one row per location, best server picked). */
export default function LocationList({
  servers,
  activeServer,
  pingingServerIds,
  searchQuery,
  onSearchChange,
  onSelect,
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

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="mb-2 flex items-center justify-between px-0.5">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-v6-muted">
          <MapPin className="h-3.5 w-3.5" strokeWidth={2.2} />
          {t('servers' as never)}
          <span className="rounded-full bg-white/[0.06] px-1.5 py-0.5 text-[9px] tabular-nums text-v6-muted">
            {groups.length}
          </span>
        </div>
      </div>

      <div className="relative mb-2">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-v6-muted" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('search' as never)}
          aria-label={t('search' as never)}
          className="v6-glass-inset w-full rounded-lg py-2 pl-8 pr-3 text-[12px] text-v6-text placeholder:text-v6-muted/70 v6-focus"
        />
      </div>

      <div role="listbox" aria-label={t('servers' as never)} className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto pr-0.5">
        {groups.length === 0 ? (
          <div className="grid flex-1 place-items-center px-4 text-center text-[11px] text-v6-muted">
            {searchQuery.trim() ? t('v6NoResults' as never) : t('addSubOrServer' as never)}
          </div>
        ) : (
          groups.map((g) => {
            const srv = g.selectedServer;
            const active =
              !!activeId &&
              g.servers.some((s) => s.id === activeId && s.subscriptionId === activeSubId);
            return (
              <ServerRow
                key={g.id}
                server={srv}
                active={active}
                pinging={g.servers.some((s) => pingingServerIds.has(s.id))}
                onSelect={onSelect}
              />
            );
          })
        )}
      </div>
    </div>
  );
}
