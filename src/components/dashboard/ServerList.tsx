import {
  Globe,
  CheckCircle2,
  ChevronDown,
  Search,
  Rss,
  RefreshCw,
  Activity,
  Loader2,
  Settings as SettingsIcon,
  Trash2,
} from 'lucide-react';
import type { ServerConfig, Subscription } from '../../stores/app-store';
import { protocolLabel } from '../../lib/utils';

interface Props {
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
  pingingServerId: string | null;
  t: (key: any) => string;
}

function renderFlag(code?: string) {
  if (!code || code.length !== 2) return <Globe className="w-4 h-4 text-text-on-orange-muted" />;
  return <img src={`https://flagcdn.com/w40/${code.toLowerCase()}.png`} alt={code} className="w-6 h-4 object-cover rounded-sm shadow-sm" />;
}

export default function ServerList({
  servers, subscriptions, activeServer, searchQuery, onSearchChange,
  collapsedGroups, onToggleGroup, onServerSelect,
  onTestSubscription, onUpdateSubscription, onRemoveSubscription,
  onTestCustomServers, onRemoveAllCustomServers, onRemoveServer,
  testingSubId, refreshingSubId, pingingServerId, t,
}: Props) {

  const renderServerItem = (server: ServerConfig) => {
    const isActive = activeServer?.id === server.id;
    const pingColor = server.ping && server.ping > 0
      ? server.ping < 100 ? 'text-emerald-600' : server.ping < 300 ? 'text-amber-600' : 'text-red-600'
      : server.ping === -1 ? 'text-red-600' : 'text-black/40';

    return (
      <button key={server.id} onClick={() => onServerSelect(server)}
        className={`w-full p-2.5 rounded-2xl flex items-center gap-3 transition-all duration-150 overflow-hidden relative cursor-pointer
          ${isActive
            ? 'bg-black text-white border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]'
            : 'bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
        <div className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 border-[2px] ${isActive ? 'bg-white border-white' : 'bg-black border-black'}`}>
          {renderFlag(server.countryCode)}
        </div>
        <div className="flex-1 text-left min-w-0 flex items-center justify-between">
          <div className="min-w-0 pr-2">
            <p className="text-sm font-black truncate tracking-tight py-0 uppercase leading-tight">{server.name}</p>
            <p className={`text-[9px] font-black uppercase tracking-widest mt-0.5 ${isActive ? 'text-emerald-400' : 'text-black/50'}`}>
              {protocolLabel(server.protocol, server.transport)}
            </p>
          </div>
          {server.ping !== undefined && (
            pingingServerId === server.id ? (
              <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
            ) : (
              <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest ${isActive ? 'text-white/80' : pingColor}`}>
                {server.ping === -1 ? t('errorLabel') : `${server.ping}ms`}
              </span>
            )
          )}
          {server.ping === undefined && pingingServerId === server.id && (
            <Loader2 className={`w-4 h-4 animate-spin shrink-0 ${isActive ? 'text-white/80' : 'text-black/40'}`} />
          )}
        </div>
        {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 shrink-0 ml-1 stroke-[3px]" />}
      </button>
    );
  };

  return (
    <div className="w-full max-w-sm mt-4 relative z-10 pb-4">
      <div className="mb-2 px-1 flex items-center justify-between">
        <span className="text-[11px] font-black text-black/50 uppercase tracking-widest pl-1">{t('activeServer')}</span>
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
        {subscriptions.map(sub => {
          const subServers = servers.filter(
            s => s.subscriptionId === sub.id &&
            (s.name.toLowerCase().includes(searchQuery.toLowerCase()) || s.countryCode?.toLowerCase() === searchQuery.toLowerCase())
          );
          if (subServers.length === 0 && searchQuery) return null;

          return (
            <div key={sub.id} className="w-full">
              {/* Subscription Header */}
              <div className="w-full flex items-center justify-between bg-white border-[3px] border-black rounded-xl p-2.5 mb-2 shadow-[2px_2px_0_#000]">
                <div className="flex items-center gap-2 min-w-0 pr-2 cursor-pointer select-none" onClick={() => onToggleGroup(sub.id)}>
                  <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups[sub.id] ? '-rotate-90' : 'rotate-0'}`} />
                  <Rss className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                  <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">{sub.name}</span>
                  <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">{sub.servers.length}</span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0 px-1">
                  <button onClick={() => onUpdateSubscription(sub)} disabled={refreshingSubId === sub.id}
                    className={`w-7 h-7 flex items-center justify-center bg-white border-[2px] border-black rounded-lg cursor-pointer text-black transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none ${refreshingSubId === sub.id ? 'opacity-70 cursor-wait' : ''}`} title={t('refreshSub')}>
                    <RefreshCw className={`w-3.5 h-3.5 stroke-[3px] ${refreshingSubId === sub.id ? 'animate-spin' : ''}`} />
                  </button>
                  <button onClick={() => onTestSubscription(sub)} disabled={testingSubId === sub.id}
                    className={`h-7 px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                      testingSubId === sub.id
                        ? 'bg-amber-400 animate-pulse text-black cursor-wait'
                        : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                    }`} title={t('testLatency')}>
                    {testingSubId === sub.id ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                    <span className="text-[10px] font-black tracking-widest uppercase">{testingSubId === sub.id ? t('testing') : t('test')}</span>
                  </button>
                  <button onClick={() => onRemoveSubscription(sub.id)}
                    className="w-7 h-7 flex items-center justify-center bg-red-400 border-[2px] border-black rounded-lg text-white cursor-pointer transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none" title={t('deleteSub')}>
                    <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                  </button>
                </div>
              </div>

              {/* Servers */}
              {!collapsedGroups[sub.id] && (
                <div className="flex flex-col gap-2 pl-2 border-l-[3px] border-black/10 ml-2 animate-slide-up">
                  {subServers.map(renderServerItem)}
                </div>
              )}
            </div>
          );
        })}

        {/* Custom / standalone servers */}
        {(() => {
          const standalone = servers.filter(
            s => !s.subscriptionId &&
            (s.name.toLowerCase().includes(searchQuery.toLowerCase()) || s.countryCode?.toLowerCase() === searchQuery.toLowerCase())
          );
          if (standalone.length === 0) return null;

          return (
            <div className="w-full mt-2">
              <div className="w-full flex items-center justify-between bg-white border-[3px] border-black rounded-xl p-2.5 mb-2 shadow-[2px_2px_0_#000]">
                <div className="flex items-center gap-2 min-w-0 pr-2 cursor-pointer select-none" onClick={() => onToggleGroup('__custom__')}>
                  <ChevronDown className={`w-4 h-4 text-black shrink-0 stroke-[3px] transition-transform duration-300 ${collapsedGroups['__custom__'] ? '-rotate-90' : 'rotate-0'}`} />
                  <SettingsIcon className="w-3.5 h-3.5 text-black shrink-0 stroke-[3px]" />
                  <span className="text-[10px] font-black text-black uppercase tracking-widest truncate">{t('customServers')}</span>
                  <span className="text-[9px] font-black bg-black text-white px-1.5 py-0.5 rounded-md uppercase tracking-widest shrink-0">{standalone.length}</span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0 px-1">
                  <button onClick={onTestCustomServers} disabled={testingSubId === '__custom__'}
                    className={`h-7 px-2.5 flex items-center justify-center gap-1 border-[2px] border-black rounded-lg cursor-pointer transition-all shadow-[2px_2px_0_#000] ${
                      testingSubId === '__custom__'
                        ? 'bg-amber-400 animate-pulse text-black cursor-wait'
                        : 'bg-emerald-400 text-black hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none'
                    }`} title={t('testLatency')}>
                    {testingSubId === '__custom__' ? <Loader2 className="w-3.5 h-3.5 stroke-[3px] animate-spin" /> : <Activity className="w-3.5 h-3.5 stroke-[3px]" />}
                    <span className="text-[10px] font-black tracking-widest uppercase">{testingSubId === '__custom__' ? t('testing') : t('test')}</span>
                  </button>
                  <button onClick={onRemoveAllCustomServers}
                    className="w-7 h-7 flex items-center justify-center bg-red-400 border-[2px] border-black rounded-lg text-white cursor-pointer transition-all shadow-[2px_2px_0_#000] hover:-translate-y-[1px] hover:-translate-x-[1px] hover:shadow-[3px_3px_0_#000] active:translate-y-[1px] active:translate-x-[1px] active:shadow-none">
                    <Trash2 className="w-3.5 h-3.5 stroke-[3px]" />
                  </button>
                </div>
              </div>
              {!collapsedGroups['__custom__'] && (
                <div className="flex flex-col gap-2 pl-2 border-l-[3px] border-black/10 ml-2">
                  {standalone.map((server) => {
                    const isActive = activeServer?.id === server.id;
                    const pingColor = server.ping && server.ping > 0
                      ? server.ping < 100 ? 'text-emerald-600' : server.ping < 300 ? 'text-amber-600' : 'text-red-600'
                      : server.ping === -1 ? 'text-red-600' : 'text-black/40';
                    return (
                      <button key={server.id} onClick={() => onServerSelect(server)}
                        className={`w-full p-2.5 rounded-2xl flex items-center gap-3 transition-all duration-150 overflow-hidden relative cursor-pointer
                          ${isActive
                            ? 'bg-black text-white border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.4)] translate-x-[-1px] translate-y-[-1px]'
                            : 'bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[4px_4px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none'}`}>
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
                          {server.ping !== undefined && (
                            <span className={`text-[10px] whitespace-nowrap font-black uppercase tracking-widest pl-1 shrink-0 ${isActive ? 'text-white/80' : pingColor}`}>
                              {server.ping === -1 ? t('errorLabel') : `${server.ping}ms`}
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                          <button onClick={(e) => { e.stopPropagation(); onRemoveServer(server.id, server.name); }}
                            className="w-8 h-8 flex items-center justify-center bg-danger/10 hover:bg-danger text-danger hover:text-white rounded-xl transition-colors cursor-pointer"
                            title={t('deleteServer')}>
                            <Trash2 className="w-4 h-4 stroke-[3px]" />
                          </button>
                          {isActive && <CheckCircle2 className="w-5 h-5 text-emerald-400 stroke-[3px]" />}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })()}
      </div>
    </div>
  );
}
