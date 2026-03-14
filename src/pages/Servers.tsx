import { useState, useCallback } from 'react';
import {
  Plus,
  Trash2,
  Zap,
  Signal,
  Globe,
  CheckCircle2,
  Rss,
  ClipboardPaste,
  RefreshCw,
  FolderTree,
  Server,
} from 'lucide-react';
import { useAppStore } from '../stores/app-store';
import { parseProxyLink } from '../lib/parser';
import { fetchSubscription, refreshSubscription } from '../lib/subscription';
import { formatPing, protocolLabel } from '../lib/utils';
import type { ServerConfig } from '../stores/app-store';

export default function Servers() {
  const {
    servers,
    activeServer,
    subscriptions,
    setActiveServer,
    addServer,
    removeServer,
    addSubscription,
    removeSubscription,
    updateSubscription,
    updateServerPing,
    addLog,
    removeAllManualServers,
  } = useAppStore();

  const [showAdd, setShowAdd] = useState(false);
  const [smartInput, setSmartInput] = useState('');
  const [testingGroup, setTestingGroup] = useState<string | null>(null);
  const [refreshingSub, setRefreshingSub] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  // Auto-detect input type
  const detectType = (input: string): 'sub' | 'link' | 'unknown' => {
    const trimmed = input.trim();
    if (!trimmed) return 'unknown';
    if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) return 'sub';
    if (/^(vless|vmess|trojan|ss|hy2|tuic|wg):\/\//.test(trimmed)) return 'link';
    return 'unknown';
  };

  const detectedType = detectType(smartInput);

  const handleSmartAdd = useCallback(async () => {
    const trimmed = smartInput.trim();
    if (!trimmed) return;
    const type = detectType(trimmed);

    if (type === 'sub') {
      setImporting(true);
      try {
        addLog('info', `Fetching subscription: ${trimmed}`);
        const sub = await fetchSubscription(trimmed);
        addSubscription(sub);
        setSmartInput('');
        setShowAdd(false);
        addLog('success', `Loaded ${sub.servers.length} servers from ${sub.name}`);
      } catch (err) {
        addLog('error', `Subscription error: ${err instanceof Error ? err.message : 'Unknown'}`);
      } finally {
        setImporting(false);
      }
    } else if (type === 'link') {
      const server = parseProxyLink(trimmed);
      if (server) {
        addServer(server);
        setSmartInput('');
        setShowAdd(false);
        addLog('success', `Added server: ${server.name}`);
      } else {
        addLog('error', 'Invalid proxy link format');
      }
    } else {
      addLog('error', 'Unrecognized format. Paste a subscription URL (https://...) or proxy link (vless://, vmess://, etc.)');
    }
  }, [smartInput, addServer, addSubscription, addLog]);

  const handlePaste = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      setSmartInput(text);
    } catch { /* */ }
  }, []);

  const handleTestGroup = useCallback(async (groupId: string, groupServers: ServerConfig[]) => {
    setTestingGroup(groupId);
    const { invoke } = await import('@tauri-apps/api/core');
    for (const server of groupServers) {
      try {
        const result: any = await invoke('ping_server', {
          address: server.address,
          port: server.port,
          serverId: server.id,
        });
        updateServerPing(server.id, result.ping_ms);
      } catch {
        updateServerPing(server.id, -1);
      }
      await new Promise((r) => setTimeout(r, 50));
    }
    setTestingGroup(null);
  }, [updateServerPing]);

  const handleRefreshSub = useCallback(async (subId: string) => {
    const sub = subscriptions.find((s) => s.id === subId);
    if (!sub) return;
    
    setRefreshingSub(subId);
    try {
      addLog('info', `Refreshing subscription: ${sub.name}`);
      const updated = await refreshSubscription(sub);
      updateSubscription(subId, updated);
      addLog('success', `Refreshed ${updated.servers.length} servers from ${updated.name}`);
    } catch (err) {
      addLog('error', `Refresh failed: ${err instanceof Error ? err.message : 'Unknown'}`);
    } finally {
      setRefreshingSub(null);
    }
  }, [subscriptions, updateSubscription, addLog]);

  const handleRemoveSub = useCallback((subId: string, subName: string) => {
    if (confirm(`Remove subscription "${subName}" and all its servers?`)) {
      removeSubscription(subId);
      addLog('info', `Removed subscription: ${subName}`);
    }
  }, [removeSubscription, addLog]);

  // Group servers
  const manualServers = servers.filter((s) => !s.subscriptionId);
  const subGroups = subscriptions.map((sub) => ({
    sub,
    servers: servers.filter((s) => s.subscriptionId === sub.id),
  }));

  const renderFlag = (code?: string) => {
    if (!code || code.length !== 2) return <Globe className="w-5 h-5 text-current opacity-70" />;
    return <img src={`https://flagcdn.com/w40/${code.toLowerCase()}.png`} alt={code} className="w-6 h-4 object-cover rounded-[2px] shadow-sm border-[1px] border-black/20" />;
  };

  // Render a single server item
  const renderServer = (server: ServerConfig) => (
    <div
      key={server.id}
      onClick={() => setActiveServer(server)}
      className={`group relative overflow-hidden rounded-2xl px-5 py-4 flex items-center gap-4 cursor-pointer transition-all duration-150 border-[3px] border-black shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-[2px_2px_0_#000]
        ${activeServer?.id === server.id 
          ? 'bg-black text-white' 
          : 'bg-white text-black'
        }
      `}
    >
      <div className={`w-8 h-8 rounded-lg flex items-center justify-center text-sm shrink-0 border-[3px] ${activeServer?.id === server.id ? 'bg-white border-white' : 'bg-black border-black text-white'}`}>
        {renderFlag(server.countryCode)}
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-sm font-black truncate uppercase tracking-tight">{server.name}</span>
          {activeServer?.id === server.id && <CheckCircle2 className="w-4 h-4 text-white shrink-0 stroke-[3px]" />}
        </div>
        <span className={`text-[10px] px-1.5 py-0.5 rounded font-black mt-1 inline-block uppercase tracking-widest border-[2px] ${activeServer?.id === server.id ? 'bg-white/20 border-transparent text-white/80' : 'bg-black/10 border-black/20 text-black/70'}`}>
          {protocolLabel(server.protocol, server.transport)}
        </span>
      </div>
      <span className={`text-sm font-black shrink-0 tracking-widest uppercase
        ${server.ping && server.ping < 100 ? 'text-emerald-600' : 
          server.ping && server.ping < 300 ? 'text-amber-600' : 
          'text-current'}`}>
        {formatPing(server.ping)}
      </span>
      
      {/* Action Buttons Container */}
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all duration-200 translate-x-4 group-hover:translate-x-0">
        <button
          onClick={(e) => { e.stopPropagation(); removeServer(server.id); }}
          className={`p-2 rounded-xl transition-all cursor-pointer border-[3px] shadow-[2px_2px_0_#000] active:translate-x-1 active:translate-y-1 active:shadow-none
            ${activeServer?.id === server.id ? 'bg-white text-danger border-white hover:bg-danger hover:text-white hover:border-black' : 'bg-black text-white border-black hover:bg-danger hover:border-black'}`}
          title="Delete server"
        >
          <Trash2 className="w-5 h-5 stroke-[3px]" />
        </button>
      </div>
    </div>
  );

  return (
    <div className="flex-1 p-5 overflow-y-auto animate-fade-in">
      <div className="max-w-2xl mx-auto space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <h1 className="text-3xl font-black text-black flex items-center gap-4 drop-shadow-[2px_2px_0_#fff] tracking-tighter uppercase">
            <span className="p-3 bg-black text-white rounded-xl shadow-[4px_4px_0_#000] border-[3px] border-black"><FolderTree className="w-6 h-6 stroke-[3px]" /></span>
            Groups & Servers
          </h1>
          <button
            onClick={() => setShowAdd(!showAdd)}
            className="flex items-center gap-2 px-5 py-3 text-sm bg-white text-black border-[3px] border-black rounded-xl font-black cursor-pointer shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-[2px_2px_0_#000] uppercase tracking-widest transition-all"
          >
            <Plus className="w-5 h-5 stroke-[4px]" /> Add
          </button>
        </div>

        {/* Add panel — smart input */}
        {showAdd && (
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000] animate-slide-up space-y-4">
            <div className="flex flex-wrap gap-3">
              <div className="flex-1 relative min-w-[200px]">
                <input
                  type="text"
                  value={smartInput}
                  onChange={(e) => setSmartInput(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && handleSmartAdd()}
                  autoFocus
                  placeholder="Paste URL or link..."
                  className="w-full bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-3 text-sm text-black placeholder:text-black/50 focus:outline-none font-black uppercase tracking-tight pr-28"
                />
                {smartInput.trim() && (
                  <span className={`absolute right-3 top-1/2 -translate-y-1/2 text-[10px] font-black px-2.5 py-1 rounded-lg border-[2px] border-black uppercase tracking-widest
                    ${detectedType === 'sub' ? 'bg-indigo-300 text-indigo-900' : detectedType === 'link' ? 'bg-emerald-300 text-emerald-900' : 'bg-red-300 text-red-900'}`}>
                    {detectedType === 'sub' ? 'Subset' : detectedType === 'link' ? 'Link' : 'Unknown'}
                  </span>
                )}
              </div>
              
              <div className="flex flex-wrap sm:flex-nowrap gap-3 shrink-0">
                <button onClick={handlePaste} className="flex items-center justify-center px-4 py-3 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none rounded-xl text-black cursor-pointer transition-all" title="Paste">
                  <ClipboardPaste className="w-5 h-5 stroke-[3px]" />
                </button>
                <button
                  onClick={handleSmartAdd}
                  disabled={importing || detectedType === 'unknown'}
                  className="flex items-center justify-center px-6 py-3 bg-black text-white border-[3px] border-black shadow-[4px_4px_0_#000] active:translate-x-[4px] active:translate-y-[4px] active:shadow-none hover:-translate-y-1 hover:-translate-x-1 hover:shadow-[6px_6px_0_#000] rounded-xl text-sm font-black uppercase tracking-widest cursor-pointer transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {importing ? '...' : 'Add'}
                </button>
              </div>
            </div>
            <p className="text-[10px] text-black/60 font-black uppercase tracking-widest px-1">
              Auto-detects: https:// → block  ·  vless:// vmess:// etc. → link
            </p>
          </div>
        )}

        {/* Empty state */}
        {servers.length === 0 && !showAdd && (
          <div className="text-center py-16 bg-white border-[4px] border-black border-dashed rounded-3xl shadow-[8px_8px_0_#000]">
            <Signal className="w-12 h-12 text-black mx-auto mb-4 stroke-[3px]" />
            <p className="text-xl font-black text-black uppercase tracking-tight">No servers found</p>
            <p className="text-[10px] text-black/60 font-black uppercase tracking-widest mt-2 max-w-xs mx-auto">
              Add a subscription link or a manual VPN link to get started.
            </p>
          </div>
        )}

        {/* Content */}
        <div className="space-y-8">
          {/* Subscriptions Groups */}
          {subGroups.map((group) => (
            <div key={group.sub.id} className="space-y-3">
              <div className="flex items-center justify-between px-2 bg-white border-[3px] border-black rounded-xl p-3 shadow-[4px_4px_0_#000]">
                <div className="flex items-center gap-3">
                  <Rss className="w-6 h-6 text-black stroke-[3px]" />
                  <h3 className="text-lg font-black text-black uppercase tracking-tight">{group.sub.name}</h3>
                  <span className="text-[10px] text-white font-black uppercase tracking-widest bg-black px-2 py-1 rounded-lg border-[2px] border-black">
                    {group.servers.length} servers
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleRefreshSub(group.sub.id)}
                    disabled={refreshingSub === group.sub.id}
                    className="p-2 border-[3px] border-black disabled:bg-black/10 rounded-xl hover:bg-black hover:text-white transition-all cursor-pointer disabled:opacity-50"
                    title="Refresh subscription"
                  >
                    <RefreshCw className={`w-5 h-5 stroke-[3px] ${refreshingSub === group.sub.id ? 'animate-spin' : ''}`} />
                  </button>
                  <button
                    onClick={() => handleTestGroup(group.sub.id, group.servers)}
                    disabled={testingGroup === group.sub.id || group.servers.length === 0}
                    className="flex items-center gap-2 px-3 py-2 border-[3px] border-black rounded-xl text-xs font-black uppercase tracking-widest bg-emerald-300 text-emerald-900 cursor-pointer hover:-translate-y-0.5 hover:shadow-[2px_2px_0_#000] active:translate-y-0.5 active:shadow-none disabled:opacity-50 transition-all"
                  >
                    <Zap className={`w-4 h-4 stroke-[3px] ${testingGroup === group.sub.id ? 'animate-pulse' : ''}`} /> Test
                  </button>
                  <button
                    onClick={() => handleRemoveSub(group.sub.id, group.sub.name)}
                    className="p-2 border-[3px] border-black rounded-xl bg-danger text-white cursor-pointer ml-2 hover:-translate-y-0.5 hover:shadow-[2px_2px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
                    title="Delete subscription"
                  >
                    <Trash2 className="w-5 h-5 stroke-[3px]" />
                  </button>
                </div>
              </div>

              {/* Server List */}
              <div className="bg-white/40 border-[4px] border-black rounded-3xl overflow-hidden p-2 space-y-2 shadow-[6px_6px_0_#000]">
                {group.servers.length === 0 ? (
                  <p className="text-sm font-black text-center text-black/40 uppercase tracking-widest py-8 px-4">No servers</p>
                ) : (
                  group.servers.map(renderServer)
                )}
              </div>
            </div>
          ))}

          {/* Manual Servers Group */}
          {manualServers.length > 0 && (
            <div className="space-y-3 mb-6">
              <div className="flex items-center justify-between px-2 bg-white border-[3px] border-black rounded-xl p-3 shadow-[4px_4px_0_#000] mt-8">
                <h3 className="text-lg font-black text-black uppercase tracking-tight flex items-center gap-3">
                  <Server className="w-6 h-6 text-black stroke-[3px]" /> Manual Servers
                </h3>
                
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => {
                        if (confirm('Are you sure you want to remove all manual servers?')) {
                            removeAllManualServers();
                            addLog('info', 'Removed all manual servers');
                        }
                    }}
                    className="p-2 border-[3px] border-black rounded-xl bg-danger text-white cursor-pointer hover:-translate-y-0.5 hover:shadow-[2px_2px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
                    title="Clear all manual servers"
                  >
                    <Trash2 className="w-5 h-5 stroke-[3px]" />
                  </button>

                  <button
                    onClick={() => handleTestGroup('manual', manualServers)}
                    disabled={testingGroup === 'manual'}
                    className="flex items-center gap-2 px-3 py-2 border-[3px] border-black rounded-xl text-xs font-black uppercase tracking-widest bg-emerald-300 text-emerald-900 cursor-pointer hover:-translate-y-0.5 hover:shadow-[2px_2px_0_#000] active:translate-y-0.5 active:shadow-none disabled:opacity-50 transition-all"
                  >
                    <Zap className={`w-4 h-4 stroke-[3px] ${testingGroup === 'manual' ? 'animate-pulse' : ''}`} /> Test
                  </button>
                </div>
              </div>
              <div className="bg-white/40 border-[4px] border-black rounded-3xl overflow-hidden p-2 space-y-2 shadow-[6px_6px_0_#000]">
                {manualServers.map(renderServer)}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
