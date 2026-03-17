import { useState, useCallback, useEffect } from 'react';
import {
  Star,
  Plus,
  Trash2,
  Shield,
  Globe,
  Ban,
  Users,
  Sparkles,
  ChevronDown,
  ChevronUp,
  FileCode,
  Wrench,
  Download,
  Upload,
  Search,
  Monitor,
} from 'lucide-react';
import { useWorkshopStore } from '../stores/workshop-store';
import type { RoutingRule } from '../stores/workshop-store';
import { useTranslation } from '../locales';

const ACTION_CONFIG = {
  proxy:  { label: 'VPN',    icon: Shield, bg: 'bg-black', text: 'text-white' },
  direct: { label: 'Direct', icon: Globe,  bg: 'bg-white border-[2px] border-black', text: 'text-black' },
  block:  { label: 'Block',  icon: Ban,    bg: 'bg-danger',  text: 'text-white border-[2px] border-black' },
};

export default function Workshop() {
  const [tab, setTab] = useState<'rules' | 'browse'>('rules');
  const init = useWorkshopStore((s) => s.init);
  const { t } = useTranslation();

  useEffect(() => {
    init();
  }, [init]);

  return (
    <div className="flex-1 p-5 overflow-y-auto animate-fade-in">
      <div className="max-w-2xl mx-auto space-y-4">
        <div className="flex items-center justify-between mb-8">
          <h1 className="text-3xl font-black text-black flex items-center gap-4 drop-shadow-[2px_2px_0_#fff] tracking-tighter uppercase">
            <span className="p-3 bg-black text-white rounded-xl shadow-[4px_4px_0_#000] border-[3px] border-black"><Wrench className="w-6 h-6 stroke-[3px]" /></span>
            {t('workshop')}
          </h1>
          <div className="flex bg-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-xl p-1 gap-1">
            <button onClick={() => setTab('rules')}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all duration-150 border-[2px]
                ${tab === 'rules' ? 'bg-black text-white border-black shadow-[2px_2px_0_rgba(0,0,0,0.5)] translate-x-[-1px] translate-y-[-1px]' : 'bg-transparent text-black border-transparent hover:bg-black/5'}`}>
              <Sparkles className="w-4 h-4 stroke-[3px]" /> {t('myRules')}
            </button>
            <button onClick={() => setTab('browse')}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all duration-150 border-[2px]
                ${tab === 'browse' ? 'bg-black text-white border-black shadow-[2px_2px_0_rgba(0,0,0,0.5)] translate-x-[-1px] translate-y-[-1px]' : 'bg-transparent text-black border-transparent hover:bg-black/5'}`}>
              <Users className="w-4 h-4 stroke-[3px]" /> {t('community')}
            </button>
          </div>
        </div>

        {tab === 'rules' ? <MyRulesTab /> : <BrowseTab />}
      </div>
    </div>
  );
}

/* =================================================================== */
/*  MY RULES — add custom domains / exe files                           */
/* =================================================================== */

function MyRulesTab() {
  const { myRules, appliedPresets, addRule, removeRule, toggleRule, setRuleAction, publishPreset, removeAppliedPreset, toggleAppliedRule, setAppliedRuleAction, removeAppliedRule } = useWorkshopStore();
  const [newType, setNewType] = useState<'domain' | 'exe'>('domain');
  const [newValue, setNewValue] = useState('');
  const [newComment, setNewComment] = useState('');
  const [newAction, setNewAction] = useState<RoutingRule['action']>('proxy');
  const [showPublish, setShowPublish] = useState(false);
  const [pubTitle, setPubTitle] = useState('');
  const [pubDesc, setPubDesc] = useState('');
  const [expandedPreset, setExpandedPreset] = useState<string | null>(null);

  const handleAdd = useCallback(() => {
    if (!newValue.trim()) return;
    addRule({
      id: crypto.randomUUID(),
      type: newType,
      value: newValue.trim(),
      action: newAction,
      enabled: true,
      comment: newComment.trim() || undefined,
    });
    setNewValue('');
    setNewComment('');
  }, [newValue, newComment, newType, newAction, addRule]);

  // ── App Scanner ──
  const [showAppScanner, setShowAppScanner] = useState(false);
  const [installedApps, setInstalledApps] = useState<{name: string, path: string}[]>([]);
  const [appSearch, setAppSearch] = useState('');
  const [scanningApps, setScanningApps] = useState(false);

  const handleScanApps = useCallback(async () => {
    setShowAppScanner(true);
    setScanningApps(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const apps: any[] = await invoke('scan_installed_apps');
      setInstalledApps(apps);
    } catch {
      setInstalledApps([]);
    } finally {
      setScanningApps(false);
    }
  }, []);

  const handleSelectApp = useCallback((app: {name: string, path: string}) => {
    // Extract just the exe filename from path
    const exeName = app.path ? app.path.split('\\').pop()?.split('/').pop() || app.name : app.name;
    setNewValue(exeName.toLowerCase());
    setNewType('exe');
    setNewComment(app.name);
    setShowAppScanner(false);
  }, []);

  const filteredApps = installedApps.filter(a =>
    a.name.toLowerCase().includes(appSearch.toLowerCase())
  );

  // ── Export/Import ──
  const handleExport = useCallback(() => {
    const allExportRules = [...myRules, ...appliedPresets.flatMap(ap => ap.rules)];
    const blob = new Blob([JSON.stringify(allExportRules, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'doodleray-rules.json';
    a.click();
    URL.revokeObjectURL(url);
  }, [myRules, appliedPresets]);

  const handleImport = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      try {
        const text = await file.text();
        const rules = JSON.parse(text);
        if (!Array.isArray(rules)) throw new Error('Invalid format');
        for (const rule of rules) {
          if (rule.type && rule.value && rule.action) {
            addRule({
              id: crypto.randomUUID(),
              type: rule.type,
              value: rule.value,
              action: rule.action,
              enabled: rule.enabled !== false,
              comment: rule.comment || undefined,
            });
          }
        }
      } catch {
        alert('Invalid rules file');
      }
    };
    input.click();
  }, [addRule]);

  const handlePublish = useCallback(() => {
    if (!pubTitle || myRules.length === 0) return;
    publishPreset({
      id: crypto.randomUUID(),
      title: pubTitle,
      description: pubDesc,
      author: 'You',
      rules: myRules,
      stars: 0,
      totalRatings: 0,
      upvotes: 0,
      hasUpvoted: false,
      createdAt: new Date().toISOString(),
    });
    setPubTitle('');
    setPubDesc('');
    setShowPublish(false);
  }, [pubTitle, pubDesc, myRules, publishPreset]);

  // Count all active rules
  const allRules = [...appliedPresets.flatMap((ap) => ap.rules), ...myRules];
  const proxyCount = allRules.filter((r) => r.action === 'proxy' && r.enabled).length;
  const directCount = allRules.filter((r) => r.action === 'direct' && r.enabled).length;
  const blockCount = allRules.filter((r) => r.action === 'block' && r.enabled).length;

  return (
    <>
      {/* Stats */}
      {allRules.length > 0 && (
        <div className="flex gap-2">
          <Stat label="VPN" count={proxyCount} />
          <Stat label="Direct" count={directCount} />
          <Stat label="Blocked" count={blockCount} />
        </div>
      )}

      {/* Applied Presets as Cards */}
      {appliedPresets.length > 0 && (
        <div className="space-y-3">
          {appliedPresets.map((ap) => {
            const isExpanded = expandedPreset === ap.presetId;
            const activeCount = ap.rules.filter((r) => r.enabled).length;
            return (
              <div key={ap.presetId} className="bg-white border-[4px] border-black shadow-[6px_6px_0_#000] rounded-2xl overflow-hidden transition-all hover:shadow-[8px_8px_0_#000]">
                {/* Preset Header */}
                <div className="px-5 py-4 flex items-center gap-4">
                  <div className="flex-1 min-w-0">
                    <h3 className="text-sm font-black text-black uppercase tracking-tight truncate">{ap.title}</h3>
                    <p className="text-[10px] font-bold text-black/50 uppercase tracking-widest mt-0.5 truncate">{ap.description}</p>
                    <div className="flex items-center gap-3 mt-2">
                      <span className="text-[9px] font-black text-black/40 uppercase tracking-widest">by {ap.author}</span>
                      <span className="text-[9px] font-black text-emerald-600 bg-emerald-50 border border-emerald-300 px-2 py-0.5 rounded-full uppercase tracking-widest">{activeCount}/{ap.rules.length} active</span>
                    </div>
                  </div>
                  <button onClick={() => setExpandedPreset(isExpanded ? null : ap.presetId)}
                    className="group flex items-center justify-center text-black bg-white border-[3px] border-black rounded-xl cursor-pointer p-2 shadow-[2px_2px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all">
                    {isExpanded ? <ChevronUp className="w-5 h-5 stroke-[3px] transition-transform duration-300 group-hover:-translate-y-0.5" /> : <ChevronDown className="w-5 h-5 stroke-[3px] transition-transform duration-300 group-hover:translate-y-0.5" />}
                  </button>
                  <button onClick={() => removeAppliedPreset(ap.presetId)}
                    className="group p-2 rounded-xl cursor-pointer border-[3px] border-black shadow-[2px_2px_0_#000] active:translate-x-1 active:translate-y-1 active:shadow-none bg-black text-white hover:bg-danger transition-all">
                    <Trash2 className="w-4 h-4 stroke-[3px] transition-transform duration-300 group-hover:scale-110 group-hover:rotate-12" />
                  </button>
                </div>

                {/* Expanded Rules */}
                {isExpanded && (
                  <div className="px-5 pb-5 border-t-[3px] border-black bg-white/50 animate-slide-up">
                    <div className="space-y-2 mt-3">
                      {ap.rules.map((rule) => (
                        <div key={rule.id} className={`group flex items-center gap-3 bg-white border-[2px] border-black shadow-[2px_2px_0_#000] rounded-xl px-4 py-2.5 transition-all ${!rule.enabled ? 'opacity-40 grayscale' : ''}`}>
                          <span className="text-lg shrink-0">{rule.type === 'exe' ? '📦' : '🌐'}</span>
                          <div className="flex-1 min-w-0">
                            <span className="text-xs font-black text-black uppercase tracking-tight block truncate">{rule.value}</span>
                            {rule.comment && <span className="text-[9px] text-black/40 font-bold tracking-widest block truncate">/* {rule.comment} */</span>}
                          </div>
                          <div className="flex gap-0.5 bg-white border-[2px] border-black rounded-lg p-0.5 shrink-0">
                            {(['proxy', 'direct', 'block'] as const).map((a) => {
                              const ac = ACTION_CONFIG[a];
                              const AIcon = ac.icon;
                              return (
                                <button key={a} onClick={() => setAppliedRuleAction(ap.presetId, rule.id, a)}
                                  className={`flex items-center gap-0.5 px-2 py-1 rounded-md text-[9px] font-black uppercase tracking-widest cursor-pointer transition-all border
                                    ${rule.action === a ? `${ac.bg} ${ac.text} border-black` : 'text-black/40 border-transparent hover:bg-black/5'}`}>
                                  <AIcon className="w-3 h-3 stroke-[3px]" />{rule.action === a && ac.label}
                                </button>
                              );
                            })}
                          </div>
                          <button onClick={() => toggleAppliedRule(ap.presetId, rule.id)}
                            className={`w-10 h-5 rounded-full relative cursor-pointer transition-all border-[2px] border-black shrink-0 ${rule.enabled ? 'bg-emerald-400' : 'bg-white'}`}>
                            <div className={`absolute top-0.5 w-3 h-3 rounded-full border-[2px] border-black transition-all ${rule.enabled ? 'left-[calc(100%-0.95rem)] bg-white' : 'left-0.5 bg-black'}`} />
                          </button>
                          <button onClick={() => removeAppliedRule(ap.presetId, rule.id)}
                            className="opacity-0 group-hover:opacity-100 p-1.5 rounded-lg cursor-pointer border-[2px] border-black bg-black text-white hover:bg-danger transition-all">
                            <Trash2 className="w-3 h-3 stroke-[3px]" />
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Add custom rule form */}
      <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-5 shadow-[6px_6px_0_#000] space-y-4">
        <p className="text-[10px] font-black uppercase tracking-widest text-black/50">+ Add Custom Rule</p>
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="flex bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl p-1 gap-1 shrink-0">
            <button onClick={() => setNewType('domain')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all border-[2px]
                ${newType === 'domain' ? 'bg-black text-white border-black' : 'text-black border-transparent hover:bg-black/5'}`}>
              <Globe className="w-4 h-4 stroke-[3px]" /> Domain
            </button>
            <button onClick={() => setNewType('exe')}
              className={`flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all border-[2px]
                ${newType === 'exe' ? 'bg-black text-white border-black' : 'text-black border-transparent hover:bg-black/5'}`}>
              <FileCode className="w-4 h-4 stroke-[3px]" /> .exe
            </button>
          </div>
          <input type="text" value={newValue} onChange={(e) => setNewValue(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
            placeholder={newType === 'domain' ? 'youtube.com, *.google.com...' : 'steam.exe, chrome.exe...'}
            className="flex-1 w-full min-w-0 bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-2 text-sm text-black placeholder:text-black/50 focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow font-black uppercase tracking-tight" />
        </div>
          <div className="flex flex-col sm:flex-row gap-3">
          <input type="text" value={newComment} onChange={(e) => setNewComment(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleAdd()}
            placeholder="Comment (optional)..."
            className="flex-1 w-full min-w-0 bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-2 text-sm text-black placeholder:text-black/50 focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow font-black uppercase tracking-tight" />
          <div className="flex flex-wrap sm:flex-nowrap gap-3 shrink-0">
            <div className="flex gap-1 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl p-1 shrink-0">
              {(['proxy', 'direct', 'block'] as const).map((a) => {
                const ac = ACTION_CONFIG[a];
                const AIcon = ac.icon;
                return (
                  <button key={a} onClick={() => setNewAction(a)}
                    className={`flex-1 flex items-center justify-center gap-1 px-3 py-1.5 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all border-[2px]
                      ${newAction === a ? `${ac.bg} ${ac.text} ${ac.bg === 'bg-black' ? 'border-black' : ''}` : 'text-black/60 border-transparent hover:bg-black/5'}`}>
                    <AIcon className="w-4 h-4 stroke-[3px]" />{newAction === a && ac.label}
                  </button>
                );
              })}
            </div>
            {newType === 'exe' && (
              <button onClick={handleScanApps}
                className="group flex items-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-xs font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all">
                <Monitor className="w-4 h-4 stroke-[3px]" /> Browse Apps
              </button>
            )}
            <button onClick={handleAdd} disabled={!newValue.trim()}
              className="group px-6 py-2 bg-black text-white border-[3px] border-black shadow-[4px_4px_0_#000] active:translate-x-[4px] active:translate-y-[4px] active:shadow-none hover:-translate-y-1 hover:-translate-x-1 hover:shadow-[6px_6px_0_#000] rounded-xl text-sm font-black cursor-pointer transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center">
              <Plus className="w-5 h-5 stroke-[4px] transition-transform duration-300 group-hover:rotate-90 group-hover:scale-110" /> Add
            </button>
          </div>
        </div>
        
        {/* Export/Import buttons */}
        <div className="flex gap-2 pt-1">
          <button onClick={handleExport}
            className="group flex items-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all">
            <Download className="w-3.5 h-3.5 stroke-[3px]" /> Export Rules
          </button>
          <button onClick={handleImport}
            className="group flex items-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all">
            <Upload className="w-3.5 h-3.5 stroke-[3px]" /> Import Rules
          </button>
        </div>
      </div>

      {/* App Scanner Modal */}
      {showAppScanner && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4" onClick={() => setShowAppScanner(false)}>
          <div className="bg-white border-[4px] border-black rounded-2xl shadow-[8px_8px_0_#000] w-full max-w-lg max-h-[70vh] flex flex-col animate-slide-up" onClick={e => e.stopPropagation()}>
            <div className="flex items-center gap-3 p-5 border-b-[3px] border-black">
              <Monitor className="w-5 h-5 stroke-[3px]" />
              <h3 className="text-lg font-black uppercase tracking-tight">Installed Apps</h3>
              <button onClick={() => setShowAppScanner(false)} className="ml-auto text-xl font-black cursor-pointer px-2">✕</button>
            </div>
            <div className="px-5 pt-4 pb-2">
              <div className="flex items-center gap-2 bg-white border-[3px] border-black shadow-inner rounded-xl px-3 py-2">
                <Search className="w-4 h-4 stroke-[3px] text-black/40" />
                <input type="text" value={appSearch} onChange={e => setAppSearch(e.target.value)}
                  placeholder="Search apps..."
                  className="flex-1 text-sm font-black text-black placeholder:text-black/40 focus:outline-none uppercase tracking-tight" />
              </div>
            </div>
            <div className="flex-1 overflow-y-auto px-5 pb-5 space-y-1">
              {scanningApps ? (
                <p className="text-center py-8 text-sm font-black text-black/40 uppercase tracking-widest animate-pulse">Scanning...</p>
              ) : filteredApps.length === 0 ? (
                <p className="text-center py-8 text-sm font-black text-black/40 uppercase tracking-widest">No apps found</p>
              ) : (
                filteredApps.map((app, i) => (
                  <button key={i} onClick={() => handleSelectApp(app)}
                    className="w-full flex items-center gap-3 px-4 py-3 bg-white border-[2px] border-black/20 rounded-xl hover:bg-black/5 hover:border-black transition-all cursor-pointer text-left">
                    <span className="text-lg">📦</span>
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-black text-black uppercase tracking-tight truncate">{app.name}</p>
                      {app.path && <p className="text-[9px] font-bold text-black/40 truncate">{app.path}</p>}
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* Custom rules list */}
      {myRules.length > 0 && (
        <div className="space-y-3 mt-4">
          <p className="text-[10px] font-black uppercase tracking-widest text-black/50 px-1">Custom Rules</p>
          {myRules.map((rule) => (
            <div key={rule.id}
              className={`group bg-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-2xl px-5 py-4 flex items-center gap-4 transition-all duration-150 hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000]
                ${!rule.enabled ? 'opacity-50 grayscale' : ''}`}>
              <div className="w-10 h-10 rounded-xl border-[3px] border-black bg-white flex items-center justify-center shrink-0">
                <span className="text-xl">{rule.type === 'domain' ? '🌐' : '📦'}</span>
              </div>
              <div className="flex-1 min-w-0">
                <p className={`text-sm font-black uppercase tracking-tight truncate transition-colors ${rule.enabled ? 'text-black' : 'text-black/60'}`}>
                  {rule.value}
                  {rule.comment && <span className="ml-3 text-[10px] lowercase text-black/50 tracking-wider">/* {rule.comment} */</span>}
                </p>
                <p className="text-[10px] font-black tracking-widest text-black/60 uppercase mt-1">{rule.type}</p>
              </div>
              <div className="flex gap-1 bg-white border-[2px] border-black shadow-[2px_2px_0_#000] rounded-xl p-1">
                {(['proxy', 'direct', 'block'] as const).map((a) => {
                  const s = ACTION_CONFIG[a];
                  const SIcon = s.icon;
                  const isActive = rule.action === a;
                  return (
                    <button key={a} onClick={() => setRuleAction(rule.id, a)}
                      className={`flex items-center gap-1 px-3 py-1.5 rounded-lg text-[10px] font-black uppercase tracking-widest cursor-pointer transition-all border-[2px]
                        ${isActive ? `${s.bg} ${s.text} ${s.bg === 'bg-black' ? 'border-black' : ''}` : 'text-black/60 border-transparent hover:bg-black/5'}`}>
                      <SIcon className="w-3.5 h-3.5 stroke-[3px]" />{isActive && s.label}
                    </button>
                  );
                })}
              </div>
              <button onClick={() => toggleRule(rule.id)}
                className={`w-12 h-6 rounded-full relative cursor-pointer transition-all border-[3px] border-black ${rule.enabled ? 'bg-emerald-400' : 'bg-white'}`}>
                <div className={`absolute top-0.5 w-3.5 h-3.5 rounded-full border-[2px] border-black transition-all ${rule.enabled ? 'left-[calc(100%-1.125rem)] bg-white' : 'left-0.5 bg-black'}`} />
              </button>
              <button onClick={() => removeRule(rule.id)}
                className="group opacity-0 group-hover:opacity-100 p-2 rounded-xl transition-all cursor-pointer border-[3px] border-black shadow-[2px_2px_0_#000] active:translate-x-1 active:translate-y-1 active:shadow-none bg-black text-white hover:bg-danger">
                <Trash2 className="w-4 h-4 stroke-[3px] transition-transform duration-300 group-hover:scale-110 group-hover:rotate-12" />
              </button>
            </div>
          ))}
        </div>
      )}

      {allRules.length === 0 && (
        <div className="text-center py-6">
          <p className="text-xs text-text-on-orange-muted">Apply a preset from Community or add custom rules above</p>
        </div>
      )}

      {/* Publish */}
      {myRules.length > 0 && (
        <div>
          {!showPublish ? (
            <button onClick={() => setShowPublish(true)}
              className="w-full mt-6 py-4 text-xs font-black uppercase tracking-widest text-black/60 hover:text-black cursor-pointer border-[3px] border-dashed border-black/30 hover:border-black rounded-2xl text-center transition-all hover:bg-white">
              Share my rules to community...
            </button>
          ) : (
            <div className="bg-white border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000] mt-6 space-y-4 animate-slide-up">
              <input type="text" value={pubTitle} onChange={(e) => setPubTitle(e.target.value)}
                placeholder="Preset Name" className="w-full bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-3 text-sm font-black text-black placeholder:text-black/50 uppercase tracking-tight focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow" />
              <textarea value={pubDesc} onChange={(e) => setPubDesc(e.target.value)}
                placeholder="Short description..." rows={2}
                className="w-full bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-3 text-xs font-black text-black placeholder:text-black/50 uppercase tracking-widest focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow resize-none" />
              <div className="flex justify-end gap-2">
                <button onClick={() => setShowPublish(false)} className="px-3 py-1.5 text-xs text-text-on-dark-muted cursor-pointer">Cancel</button>
                <button onClick={handlePublish} disabled={!pubTitle}
                  className="px-4 py-1.5 text-xs bg-bg-primary text-el-primary rounded-lg font-bold cursor-pointer disabled:opacity-40">Publish</button>
              </div>
            </div>
          )}
        </div>
      )}
    </>
  );
}

/* =================================================================== */
/*  BROWSE — Community presets                                          */
/* =================================================================== */

function BrowseTab() {
  const { presets, sortBy, setSortBy, applyPreset, ratePreset, comments, addComment, loadComments } = useWorkshopStore();
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [appliedId, setAppliedId] = useState<string | null>(null);
  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [reviewStars, setReviewStars] = useState(5);
  const [reviewHover, setReviewHover] = useState(0);
  const [reviewText, setReviewText] = useState('');
  const [starHover, setStarHover] = useState<{id: string, n: number} | null>(null);

  const handleExpand = (id: string) => {
    const newId = expandedId === id ? null : id;
    setExpandedId(newId);
    if (newId) loadComments(newId);
  };

  const handleApply = (id: string) => {
    applyPreset(id);
    setAppliedId(id);
    setTimeout(() => setAppliedId(null), 2000);
  };

  const handleQuickRate = (presetId: string, stars: number) => {
    ratePreset(presetId, stars);
  };

  const handleSubmitReview = (presetId: string) => {
    ratePreset(presetId, reviewStars);
    if (reviewText.trim()) {
      addComment(presetId, reviewText.trim(), reviewStars);
    }
    setReviewingId(null);
    setReviewText('');
    setReviewStars(5);
  };

  const sorted = [...presets].sort((a, b) => {
    if (sortBy === 'popular') return b.stars - a.stars;
    if (sortBy === 'top-rated') return b.stars - a.stars;
    return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
  });

  return (
    <>
      <div className="flex justify-end mb-6 mt-4">
        <div className="flex bg-white border-[3px] border-black rounded-xl p-1 gap-1 shadow-[4px_4px_0_#000]">
          {(['popular', 'newest', 'top-rated'] as const).map((s) => (
            <button key={s} onClick={() => setSortBy(s)}
              className={`px-3 py-1.5 text-[10px] rounded-lg font-black uppercase tracking-widest cursor-pointer transition-all border-[2px]
                ${sortBy === s ? 'bg-black text-white border-black shadow-[2px_2px_0_rgba(0,0,0,0.5)] translate-x-[-1px] translate-y-[-1px]' : 'bg-transparent text-black border-transparent hover:bg-black/5'}`}>{s.replace('-', ' ')}</button>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        {sorted.map((preset) => {
          const isExpanded = expandedId === preset.id;
          const proxyR = preset.rules.filter((r) => r.action === 'proxy').length;
          const directR = preset.rules.filter((r) => r.action === 'direct').length;
          const blockR = preset.rules.filter((r) => r.action === 'block').length;
          const isReviewing = reviewingId === preset.id;
          const presetComments = comments[preset.id] || [];
          const hoverStar = starHover?.id === preset.id ? starHover.n : 0;

          return (
            <div key={preset.id} className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] overflow-hidden transition-all hover:-translate-y-1 hover:shadow-[8px_8px_0_#000]">
              <div className="px-6 py-5">
                <div className="flex items-start gap-4">
                  <div className="flex-1 min-w-0">
                    <h3 className="text-xl font-black text-black uppercase tracking-tight">{preset.title}</h3>
                    <p className="text-xs font-black text-black/70 tracking-widest uppercase mt-1">{preset.description}</p>

                    <div className="flex flex-wrap gap-2 mt-4">
                      {preset.rules.slice(0, 5).map((r) => (
                        <span key={r.id} className={`text-[10px] px-2 py-1 rounded-lg font-black uppercase tracking-widest border-[2px]
                          ${r.action === 'proxy' ? 'bg-black text-white border-black' : r.action === 'direct' ? 'bg-white text-black border-black' : 'bg-danger text-white border-black'}`}>
                          {r.type === 'exe' ? '📦 ' : ''}{r.value}
                        </span>
                      ))}
                      {preset.rules.length > 5 && <span className="text-[10px] font-black text-black/60 uppercase tracking-widest self-center">+{preset.rules.length - 5} MORE</span>}
                    </div>

                    <div className="flex items-center gap-3 mt-5 flex-wrap">
                      <span className="text-xs font-black uppercase tracking-widest text-black/70 bg-black/5 px-2 py-1 rounded-lg border-2 border-black/10">🛡️ {proxyR} · 🌐 {directR}{blockR > 0 ? ` · 🚫 ${blockR}` : ''}</span>
                      
                      {/* Clickable Yellow Stars */}
                      <div className="flex items-center gap-0.5 bg-white border-2 border-black rounded-lg px-2.5 py-1.5 shadow-[2px_2px_0_#000]">
                        {[1,2,3,4,5].map((n) => (
                          <button key={n}
                            onMouseEnter={() => setStarHover({id: preset.id, n})}
                            onMouseLeave={() => setStarHover(null)}
                            onClick={() => handleQuickRate(preset.id, n)}
                            className="cursor-pointer hover:scale-110 active:scale-90 transition-transform">
                            <Star className={`w-5 h-5 stroke-[2px] transition-colors ${
                              n <= (hoverStar || Math.round(preset.stars)) ? 'text-amber-400 fill-amber-400' : 'text-black/15'
                            }`} />
                          </button>
                        ))}
                        <span className="text-xs font-black text-black ml-1.5">{preset.stars > 0 ? preset.stars.toFixed(1) : '—'}</span>
                        <span className="text-[9px] font-black text-black/40 ml-0.5">({preset.totalRatings})</span>
                      </div>

                      {/* Write Review */}
                      <button onClick={() => { setReviewingId(isReviewing ? null : preset.id); setReviewStars(preset.myRating || 5); setReviewText(''); setReviewHover(0); }}
                        className={`flex items-center gap-1.5 px-3 py-1.5 text-[10px] font-black uppercase tracking-widest border-[2px] rounded-lg cursor-pointer transition-all
                          ${isReviewing ? 'bg-amber-400 text-black border-black shadow-none' : 'bg-white text-black border-black shadow-[2px_2px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000]'}`}>
                        ✍️ Review
                      </button>

                      <span className="text-[10px] font-black uppercase tracking-widest text-black/50 ml-auto">BY {preset.author}</span>
                    </div>

                    {/* Review Form */}
                    {isReviewing && (
                      <div className="mt-4 p-4 bg-amber-50 border-[3px] border-black rounded-xl shadow-[4px_4px_0_#000] animate-slide-up">
                        <p className="text-xs font-black uppercase tracking-widest text-black/60 mb-3">Write a Review</p>
                        <div className="flex items-center gap-1 mb-3">
                          {[1,2,3,4,5].map((n) => (
                            <button key={n}
                              onMouseEnter={() => setReviewHover(n)}
                              onMouseLeave={() => setReviewHover(0)}
                              onClick={() => setReviewStars(n)}
                              className="cursor-pointer hover:scale-125 active:scale-95 transition-transform p-0.5">
                              <Star className={`w-7 h-7 stroke-[2px] transition-colors ${n <= (reviewHover || reviewStars) ? 'text-amber-400 fill-amber-400' : 'text-black/20'}`} />
                            </button>
                          ))}
                          <span className="text-sm font-black text-black ml-2">{reviewHover || reviewStars}/5</span>
                        </div>
                        <textarea value={reviewText} onChange={(e) => setReviewText(e.target.value)}
                          placeholder="Write a comment (optional)..."
                          rows={2}
                          className="w-full bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-2 text-xs font-bold text-black placeholder:text-black/40 focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow resize-none tracking-tight" />
                        <div className="flex justify-end gap-2 mt-3">
                          <button onClick={() => setReviewingId(null)}
                            className="px-4 py-2 text-[10px] font-black uppercase tracking-widest text-black/50 hover:text-black cursor-pointer">Cancel</button>
                          <button onClick={() => handleSubmitReview(preset.id)}
                            className="px-5 py-2 bg-black text-white border-[3px] border-black shadow-[3px_3px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[5px_5px_0_#000] active:translate-y-1 active:shadow-none transition-all">
                            Submit
                          </button>
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="flex flex-col gap-2 shrink-0">
                    <button onClick={() => handleApply(preset.id)}
                      className={`px-5 py-3 text-sm border-[3px] rounded-xl font-black uppercase tracking-widest cursor-pointer shadow-[2px_2px_0_#000] hover:-translate-y-1 hover:shadow-[4px_4px_0_#000] active:translate-y-1 active:shadow-none transition-all ${
                        appliedId === preset.id ? 'bg-emerald-400 text-black border-black' : 'bg-black text-white border-black'
                      }`}>
                      {appliedId === preset.id ? 'Applied! ✓' : 'Apply'}
                    </button>
                    <button onClick={() => handleExpand(preset.id)}
                      className="group flex items-center justify-center text-black bg-white border-[3px] border-black rounded-xl cursor-pointer p-2 shadow-[2px_2px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all">
                      {isExpanded ? <ChevronUp className="w-5 h-5 stroke-[3px] transition-transform duration-300 group-hover:-translate-y-0.5" /> : <ChevronDown className="w-5 h-5 stroke-[3px] transition-transform duration-300 group-hover:translate-y-0.5" />}
                    </button>
                  </div>
                </div>
              </div>

              {isExpanded && (
                <div className="px-6 pb-6 border-t-[3px] border-black bg-white/50 animate-slide-up">
                  {/* Rules */}
                  <div className="grid grid-cols-2 gap-3 mt-4">
                    {preset.rules.map((r) => {
                      const ac = ACTION_CONFIG[r.action];
                      return (
                        <div key={r.id} className="flex items-center gap-3 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl px-4 py-2 hover:translate-x-[-1px] hover:translate-y-[-1px] hover:shadow-[4px_4px_0_#000] transition-all">
                          <span className="text-lg">{r.type === 'exe' ? '📦' : '🌐'}</span>
                          <div className="flex-1 min-w-0">
                            <span className="text-xs text-black font-black uppercase tracking-tight block truncate">{r.value}</span>
                            {r.comment && <span className="text-[9px] text-black/50 font-black tracking-widest lowercase block truncate">/* {r.comment} */</span>}
                          </div>
                          <span className={`text-[9px] px-2 py-1 rounded-lg font-black uppercase tracking-widest ${ac.bg} ${ac.text} border-2 ${ac.bg === 'bg-black' ? 'border-black' : ''}`}>{ac.label}</span>
                        </div>
                      );
                    })}
                  </div>

                  {/* Comments */}
                  <div className="mt-6 border-t-[3px] border-black/10 pt-4">
                    <h4 className="text-sm font-black uppercase tracking-widest text-black/60 mb-3">💬 Reviews ({presetComments.length})</h4>
                    {presetComments.length === 0 ? (
                      <p className="text-xs font-bold text-black/30 uppercase tracking-widest py-4 text-center">No reviews yet — be the first!</p>
                    ) : (
                      <div className="space-y-3">
                        {presetComments.map((c, i) => (
                          <div key={i} className="bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl px-4 py-3">
                            <div className="flex items-center gap-2 mb-1">
                              <span className="text-xs font-black text-black uppercase tracking-tight">{c.nickname}</span>
                              <div className="flex gap-0.5">
                                {[1,2,3,4,5].map(n => (
                                  <Star key={n} className={`w-3 h-3 stroke-[2px] ${n <= c.stars ? 'text-amber-400 fill-amber-400' : 'text-black/15'}`} />
                                ))}
                              </div>
                              <span className="text-[9px] font-bold text-black/30 ml-auto">{c.date}</span>
                            </div>
                            <p className="text-xs text-black/70 font-bold">{c.text}</p>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </>
  );
}




function Stat({ label, count }: { label: string; count: number }) {
  return (
    <div className="flex-1 bg-bg-primary border-[4px] border-black rounded-2xl px-4 py-3 text-center shadow-[4px_4px_0_#000]">
      <p className="text-2xl font-black text-black drop-shadow-[2px_2px_0_#fff]">{count}</p>
      <p className="text-[10px] text-black/60 font-black uppercase tracking-widest mt-1">{label}</p>
    </div>
  );
}
