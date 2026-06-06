import {
  Globe,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Search,
  Rss,
  RefreshCw,
  Activity,
  AlertTriangle,
  Loader2,
  Settings as SettingsIcon,
  Trash2,
} from 'lucide-react';
import { useMemo, type ReactNode } from 'react';
import type { ConnectionStatus, ServerConfig, Subscription } from '../../stores/app-store';
import { formatBytes, protocolLabel } from '../../lib/utils';
import { buildServerDisplayGroups, serverMatchesGroupQuery, type ServerDisplayGroup } from '../../lib/server-groups';
import { buildServerSelectionIndex, findMatchingServerInIndex } from '../../lib/server-selection';
import { getSubscriptionTrafficStatus } from '../../lib/subscription-status';

interface Props {
  status: ConnectionStatus;
  servers: ServerConfig[];
  subscriptions: Subscription[];
  activeServer: ServerConfig | null;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  collapsedGroups: Record<string, boolean>;
  onToggleGroup: (id: string) => void;
  onServerSelect: (server: ServerConfig) => void;
  onTestSubscription: (sub: Subscription) => void;
  onUpdateSubscription: (sub: Subscription) => void;
  onRemoveSubscription: (id: string) => void;
  onTestCustomServers: () => void;
  onRemoveAllCustomServers: () => void;
  onRemoveServer: (serverId: string, serverName: string) => void;
  testingSubId: string | null;
  refreshingSubId: string | null;
  pingingServerIds: Set<string>;
  subAutoUpdateMinutes: number;
  t: (key: any) => string;
}

function renderFlag(code?: string) {
  if (!code || code.length !== 2) return <Globe className="w-5 h-5 text-white/80" />;
  return <img src={`https://flagcdn.com/w40/${code.toLowerCase()}.png`} alt={code} className="w-6 h-4 object-cover rounded-sm shadow-sm" />;
}

function serverProtocolLabel(server: ServerConfig) {
  return server.rawConfig ? `${server.protocol.toUpperCase()} | JSON` : protocolLabel(server.protocol, server.transport);
}

function groupPingColor(ping: number | undefined): string {
  if (ping === undefined) return 'text-black/35';
  if (ping < 0) return 'text-red-600';
  if (ping < 100) return 'text-emerald-600';
  if (ping < 300) return 'text-amber-600';
  return 'text-red-600';
}

function formatTrafficQuotaBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024 * 1024) return formatBytes(bytes);

  const gb = bytes / (1024 * 1024 * 1024);
  const isWholeGb = Math.abs(gb - Math.round(gb)) < 0.001;
  return `${isWholeGb ? Math.round(gb).toString() : gb.toFixed(2)} GB`;
}

function formatAutoUpdateInterval(minutes: number, t: (key: any) => string): string | null {
  if (minutes <= 0) return null;
  if (minutes % 60 === 0) return `${minutes / 60} ${t('hoursShort')}`;
  return `${minutes} ${t('minutesShort')}`;
}

function CollapsibleSection({
  open,
  children,
  className = '',
}: {
  open: boolean;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      data-open={open ? 'true' : 'false'}
      aria-hidden={!open}
      inert={!open ? true : undefined}
      style={!open ? { height: 0, maxHeight: 0, opacity: 0, overflow: 'hidden' } : undefined}
      className={`smooth-collapse ${className}`}
    >
      <div className="smooth-collapse-inner">
        {children}
      </div>
    </div>
  );
}

export default function ServerList({
  status, servers, subscriptions, activeServer, searchQuery, onSearchChange,
  collapsedGroups, onToggleGroup, onServerSelect,
  onTestSubscription, onUpdateSubscription, onRemoveSubscription,
  onTestCustomServers, onRemoveAllCustomServers, onRemoveServer,
  testingSubId, refreshingSubId, pingingServerIds, subAutoUpdateMinutes, t,
}: Props) {
  const selectionIndex = useMemo(() => buildServerSelectionIndex(servers), [servers]);
  const visibleActiveServer = findMatchingServerInIndex(activeServer, selectionIndex) || activeServer;
  const { subscriptionRows, standalone } = useMemo(() => {
    const allBySubscription = new Map<string, ServerConfig[]>();
    const visibleBySubscription = new Map<string, ServerConfig[]>();
    const visibleStandalone: ServerConfig[] = [];

    for (const server of servers) {
      const matchesQuery = serverMatchesGroupQuery(server, searchQuery);
      if (!server.subscriptionId) {
        if (matchesQuery) visibleStandalone.push(server);
        continue;
      }

      const allServers = allBySubscription.get(server.subscriptionId);
      if (allServers) {
        allServers.push(server);
      } else {
        allBySubscription.set(server.subscriptionId, [server]);
      }

      if (matchesQuery) {
        const visibleServers = visibleBySubscription.get(server.subscriptionId);
        if (visibleServers) {
          visibleServers.push(server);
        } else {
          visibleBySubscription.set(server.subscriptionId, [server]);
        }
      }
    }

    return {
      subscriptionRows: subscriptions.map((sub) => ({
        sub,
        subServerGroups: buildServerDisplayGroups(visibleBySubscription.get(sub.id) || []),
        subGroupCount: buildServerDisplayGroups(allBySubscription.get(sub.id) || []).length,
        trafficStatus: getSubscriptionTrafficStatus(sub),
      })),
      standalone: visibleStandalone,
    };
  }, [servers, subscriptions, searchQuery]);
  const showCurrentServer = status !== 'disconnected' && !!visibleActiveServer;
  const activeServerPingText = visibleActiveServer?.ping === undefined
    ? null
    : visibleActiveServer.ping < 0
      ? t('errorLabel')
      : `tcp ${visibleActiveServer.ping}ms`;

  const renderServerGroup = (group: ServerDisplayGroup) => {
    const activeGroupServer = visibleActiveServer && group.servers.some((server) => server.id === visibleActiveServer.id)
      ? visibleActiveServer
      : null;
    const selectedServer = activeGroupServer || group.selectedServer;
    const isActive = !!activeGroupServer;
    const isPinging = group.servers.some((server) => pingingServerIds.has(server.id));
    const pingText = group.ping === undefined
      ? null
      : group.ping < 0
        ? t('errorLabel')
        : `tcp ${group.ping}ms`;

    return (
      <button key={group.id} onClick={() => onServerSelect(selectedServer)}
        className={`mr-1 w-[calc(100%-0.25rem)] min-h-[64px] p-2.5 pr-3 rounded-2xl flex items-center gap-3 transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] overflow-visible relative cursor-pointer
          ${isActive
            ? 'bg-black text-white border-[3px] border-emerald-400 shadow-[0_0_0_3px_rgba(52,211,153,0.18),4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]'
            : 'bg-white/92 text-black border-[2px] border-black shadow-[1px_1px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
        <div className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border-[2px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
          {renderFlag(group.countryCode)}
        </div>
        <div className="flex-1 text-left min-w-0 flex items-center justify-between">
          <div className="min-w-0 pr-2">
            <p className="text-sm font-black truncate tracking-tight py-0 uppercase leading-tight">{group.label}</p>
            <p className={`text-[9px] font-black uppercase tracking-widest mt-0.5 ${isActive ? 'text-emerald-400' : 'text-black/50'}`}>
              {serverProtocolLabel(selectedServer)}
            </p>
          </div>
          {isPinging ? (
            <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
          ) : (
            <div className="flex items-center gap-1.5 shrink-0">
              {pingText && (
                <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest ${isActive ? 'text-white/80' : groupPingColor(group.ping)}`}>
                  {pingText}
                </span>
              )}
              <ChevronRight className={`w-5 h-5 shrink-0 stroke-[3px] ${isActive ? 'text-white/70' : 'text-black/35'}`} />
            </div>
          )}
        </div>
        {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 shrink-0 ml-1 stroke-[3px]" />}
      </button>
    );
  };

  const renderSubscriptionUsage = (sub: Subscription) => {
    const trafficStatus = getSubscriptionTrafficStatus(sub);
    const expire = sub.traffic?.expire
      ? new Date(sub.traffic.expire * 1000).toLocaleDateString('ru-RU')
      : null;
    const updated = sub.updatedAt
      ? new Date(sub.updatedAt).toLocaleDateString('ru-RU')
      : null;
    const autoUpdateInterval = formatAutoUpdateInterval(subAutoUpdateMinutes, t);
    const quotaFillClass = trafficStatus.isLimited
      ? 'bg-red-500'
      : trafficStatus.remainingPercent <= 15
        ? 'bg-amber-400'
        : 'bg-emerald-400';

    return (
      <div className="mt-2 border-t-[2px] border-black/10 pt-2">
        {trafficStatus.isLimited && (
          <div className="mb-2 flex items-center gap-1.5 rounded-lg border-[2px] border-black bg-amber-300 px-2 py-1 text-[8px] font-black uppercase tracking-widest text-black">
            <AlertTriangle className="h-3.5 w-3.5 shrink-0 stroke-[3px]" />
            <span className="truncate">
              {trafficStatus.reason === 'expired' ? t('subscriptionExpired') : t('subscriptionLimited')}
            </span>
          </div>
        )}
        {trafficStatus.hasQuota ? (
          <>
            <div className="h-2 overflow-hidden rounded-full border-[2px] border-black bg-black/10">
              <div className={`h-full ${quotaFillClass}`} style={{ width: `${trafficStatus.usedPercent}%` }} />
            </div>
            <div className="mt-1.5 flex min-w-0 flex-wrap items-center justify-between gap-x-2 gap-y-0.5 text-[8px] font-black uppercase tracking-widest text-black/55">
              <span className="truncate">
                {`${formatTrafficQuotaBytes(trafficStatus.used)} / ${formatTrafficQuotaBytes(trafficStatus.total)}`}
              </span>
              {expire && <span className="shrink-0">{t('validUntil')} {expire}</span>}
            </div>
          </>
        ) : (
          <p className="text-[8px] font-black uppercase tracking-widest text-black/40">
            {t('trafficUnavailable')}
          </p>
        )}
        {updated && (
          <p className="mt-1 break-words text-[8px] font-black uppercase tracking-widest text-black/35">
            {t('lastUpdated')} {updated}
            {autoUpdateInterval && <> | {t('autoUpdateLabel')} {autoUpdateInterval}</>}
          </p>
        )}
      </div>
    );
  };

  return (
    <div className="w-full max-w-md mt-4 relative z-10 pb-4">
      {showCurrentServer && visibleActiveServer && (
        <div className="mb-3 w-full rounded-2xl border-[3px] border-black bg-black p-3 text-white shadow-[5px_5px_0_rgba(0,0,0,0.35)]">
          <div className="mb-2 flex items-center justify-between gap-3">
            <span className="text-[10px] font-black uppercase tracking-widest text-white/55">
              {t('activeServer')}
            </span>
            <span className={`rounded-lg border-[2px] border-emerald-400 px-2 py-0.5 text-[9px] font-black uppercase tracking-widest ${
              status === 'connected' ? 'bg-emerald-400 text-black' : 'bg-amber-300 text-black'
            }`}>
              {status === 'connected' ? t('connected') : status === 'disconnecting' ? t('disconnecting') : t('connecting')}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border-[2px] border-white bg-white">
              {renderFlag(visibleActiveServer.countryCode)}
            </div>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-black uppercase leading-tight tracking-tight">
                {visibleActiveServer.name}
              </p>
              <div className="mt-1 flex min-w-0 items-center gap-2 text-[9px] font-black uppercase tracking-widest text-white/55">
                <span className="truncate">{serverProtocolLabel(visibleActiveServer)}</span>
                {activeServerPingText && <span className="shrink-0 text-emerald-300">{activeServerPingText}</span>}
              </div>
            </div>
          </div>
        </div>
      )}

      <div className="mb-2 px-1 flex items-center justify-between">
        <span className="text-[11px] font-black text-black/50 uppercase tracking-widest pl-1">{t('servers')}</span>
        {servers.length > 5 && (
          <div className="relative w-32">
            <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-black/50 stroke-[3px]" />
            <input type="text" placeholder={t('search')}
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              className="w-full bg-black/5 rounded-lg pl-7 pr-2 py-1 text-[10px] font-black text-black focus:outline-none placeholder:text-black/30 uppercase tracking-widest focus:bg-white focus:border-black border-[2px] border-transparent" />
          </div>
        )}
      </div>

      <div className="flex flex-col gap-4 relative z-20">
        {/* Subscription groups */}
        {subscriptionRows.map(({ sub, subServerGroups, subGroupCount, trafficStatus }) => {
          if (subServerGroups.length === 0 && searchQuery) return null;

          return (
            <div key={sub.id} className="w-full">
              {/* Subscription Header */}
              <div className="w-full bg-white/90 border-[2px] border-black/70 rounded-xl p-2.5 mb-2 shadow-[1px_1px_0_rgba(0,0,0,0.35)] backdrop-blur transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]">
                <div className="flex w-full min-w-0 items-center justify-between gap-2">
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 cursor-pointer appearance-none items-center gap-2 bg-transparent pr-1 text-left select-none"
                    onClick={() => onToggleGroup(sub.id)}
                  >
                    <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups[sub.id] ? '-rotate-90' : 'rotate-0'}`} />
                    <Rss className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                    <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">{sub.name}</span>
                    <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">{subGroupCount}</span>
                    {trafficStatus.isLimited && (
                      <span className="hidden sm:inline-flex rounded-md border-[2px] border-black bg-amber-300 px-1.5 py-0.5 text-[8px] font-black uppercase tracking-widest text-black shrink-0">
                        {t('subscriptionLimitedShort')}
                      </span>
                    )}
                  </button>
                  <div className="ml-auto flex shrink-0 items-center gap-1">
                    <button onClick={() => onUpdateSubscription(sub)} disabled={refreshingSubId === sub.id}
                      className={`w-7 h-7 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer text-black transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none ${refreshingSubId === sub.id ? 'opacity-70 cursor-wait' : ''}`} title={t('refreshSub')}>
                      <RefreshCw className={`w-3.5 h-3.5 stroke-[3px] ${refreshingSubId === sub.id ? 'animate-spin' : ''}`} />
                    </button>
                    <button onClick={() => onTestSubscription(sub)} disabled={testingSubId === sub.id}
                      className={`h-7 w-7 md:w-auto md:px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                        testingSubId === sub.id
                          ? 'bg-amber-400 animate-pulse text-black cursor-wait'
                          : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                      }`} title={t('testLatency')}>
                      {testingSubId === sub.id ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                      <span className="hidden text-[10px] font-black tracking-widest uppercase md:inline">{testingSubId === sub.id ? t('testing') : t('test')}</span>
                    </button>
                    <button onClick={() => onRemoveSubscription(sub.id)}
                      className="w-7 h-7 flex items-center justify-center bg-white border-[2px] border-black rounded-lg text-danger cursor-pointer transition-all shadow-[1px_1px_0_#000] hover:bg-danger hover:text-white hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[2px_2px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none" title={t('deleteSub')}>
                      <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                    </button>
                  </div>
                </div>
                <CollapsibleSection open={!collapsedGroups[sub.id]}>
                  {renderSubscriptionUsage(sub)}
                </CollapsibleSection>
              </div>

              {/* Servers */}
              <CollapsibleSection open={!collapsedGroups[sub.id]}>
                <div className="ml-2 flex flex-col gap-2 overflow-visible border-l-[3px] border-black/10 pl-2 pr-1 py-0.5">
                  {subServerGroups.map(renderServerGroup)}
                </div>
              </CollapsibleSection>
            </div>
          );
        })}

        {/* Custom / standalone servers */}
        {(() => {
          if (standalone.length === 0) return null;

          return (
            <div className="w-full mt-2">
              <div className="flex w-full min-w-0 items-center justify-between gap-2 bg-white/90 border-[2px] border-black/70 rounded-xl p-2.5 mb-2 shadow-[1px_1px_0_rgba(0,0,0,0.35)] backdrop-blur transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]">
                <div className="flex min-w-0 flex-1 items-center gap-2 pr-1 cursor-pointer select-none" onClick={() => onToggleGroup('__custom__')}>
                  <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups['__custom__'] ? '-rotate-90' : 'rotate-0'}`} />
                  <SettingsIcon className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                  <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">{t('customServers')}</span>
                  <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">{standalone.length}</span>
                </div>
                <div className="ml-auto flex shrink-0 items-center gap-1">
                  <button onClick={onTestCustomServers} disabled={testingSubId === '__custom__'}
                    className={`h-7 w-7 md:w-auto md:px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                      testingSubId === '__custom__'
                        ? 'bg-amber-400 animate-pulse text-black cursor-wait'
                        : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                    }`} title={t('testLatency')}>
                    {testingSubId === '__custom__' ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                    <span className="hidden text-[10px] font-black tracking-widest uppercase md:inline">{testingSubId === '__custom__' ? t('testing') : t('test')}</span>
                  </button>
                  <button onClick={onRemoveAllCustomServers}
                    className="w-7 h-7 flex items-center justify-center bg-white border-[2px] border-black rounded-lg text-danger cursor-pointer transition-all shadow-[1px_1px_0_#000] hover:bg-danger hover:text-white hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[2px_2px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none">
                    <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                  </button>
                </div>
              </div>
              <CollapsibleSection open={!collapsedGroups['__custom__']}>
                <div className="ml-2 flex flex-col gap-2 overflow-visible border-l-[3px] border-black/10 pl-2 pr-1 py-0.5">
                  {standalone.map((server) => {
                    const isActive = visibleActiveServer?.id === server.id;
                    const isPinging = pingingServerIds.has(server.id);
                    const pingColor = server.ping && server.ping > 0
                      ? server.ping < 100 ? 'text-emerald-600' : server.ping < 300 ? 'text-amber-600' : 'text-red-600'
                      : server.ping === -1 ? 'text-red-600' : 'text-black/40';
                    return (
                      <div
                        key={server.id}
                        role="button"
                        tabIndex={0}
                        onClick={() => onServerSelect(server)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            onServerSelect(server);
                          }
                        }}
                        className={`mr-1 w-[calc(100%-0.25rem)] min-h-[64px] p-2.5 pr-3 rounded-2xl flex items-center gap-3 transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] overflow-visible relative cursor-pointer
                          ${isActive
                            ? 'bg-black text-white border-[3px] border-emerald-400 shadow-[0_0_0_3px_rgba(52,211,153,0.18),4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]'
                            : 'bg-white/92 text-black border-[2px] border-black shadow-[1px_1px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
                        <div className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border-[2px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
                          {renderFlag(server.countryCode)}
                        </div>
                        <div className="flex-1 text-left min-w-0 flex items-center justify-between mr-1">
                          <div className="min-w-0 pr-1 truncate">
                            <p className="text-sm font-black truncate tracking-tight py-0 uppercase leading-tight">{server.name}</p>
                            <p className={`text-[9px] font-black uppercase tracking-widest mt-0.5 ${isActive ? 'text-emerald-400' : 'text-black/50'}`}>
                              {protocolLabel(server.protocol, server.transport)}
                            </p>
                          </div>
                          {isPinging ? (
                            <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
                          ) : server.ping !== undefined && (
                            <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest pl-1 shrink-0 ${isActive ? 'text-white/80' : pingColor}`}>
                              {server.ping === -1 ? t('errorLabel') : `tcp ${server.ping}ms`}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          <button onClick={(e) => { e.stopPropagation(); onRemoveServer(server.id, server.name); }}
                            className={`w-8 h-8 flex items-center justify-center rounded-xl transition-colors cursor-pointer ${
                              isActive
                                ? 'bg-white/10 text-white/35 hover:bg-danger hover:text-white'
                                : 'bg-danger/10 text-danger hover:bg-danger hover:text-white'
                            }`}
                            title={t('deleteServer')}>
                            <Trash2 className="w-4 h-4 stroke-[3px]" />
                          </button>
                          {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 stroke-[3px]" />}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </CollapsibleSection>
            </div>
          );
        })()}
      </div>
    </div>
  );
}
