import { useState, useCallback, useEffect } from 'react';
import {
  Ban,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  FileCode,
  Globe,
  Monitor,
  Plus,
  RefreshCw,
  Search,
  Shield,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Upload,
  Users,
  Wrench,
} from 'lucide-react';
import { useWorkshopStore } from '../stores/workshop-store';
import type { RoutingPreset, RoutingRule } from '../stores/workshop-store';
import { useTranslation } from '../locales';
import { desktopBridge } from '../platform/tauri/desktop-bridge';

const ACTION_ORDER: RoutingRule['action'][] = ['proxy', 'direct', 'block'];

const ACTION_CONFIG: Record<RoutingRule['action'], {
  label: string;
  shortLabel: string;
  icon: typeof Shield;
  bg: string;
  text: string;
}> = {
  proxy: {
    label: 'Через VPN',
    shortLabel: 'VPN',
    icon: Shield,
    bg: 'bg-black',
    text: 'text-white border-black',
  },
  direct: {
    label: 'Напрямую',
    shortLabel: 'Прямо',
    icon: Globe,
    bg: 'bg-white',
    text: 'text-black border-black',
  },
  block: {
    label: 'Блокировать',
    shortLabel: 'Блок',
    icon: Ban,
    bg: 'bg-danger',
    text: 'text-white border-black',
  },
};

const SORT_LABELS: Record<'popular' | 'newest' | 'top-rated', string> = {
  popular: 'Популярные',
  newest: 'Новые',
  'top-rated': 'Лучшие',
};

function isGamingMinPingPreset(preset: Pick<RoutingPreset, 'id' | 'title'> | { presetId: string; title: string }) {
  const id = 'id' in preset ? preset.id : preset.presetId;
  return id === 'builtin-gaming-direct' ||
    id === 'builtin-gaming-min-ping';
}

function isPresetApplied(appliedPresets: Array<{ presetId: string; title: string }>, preset: RoutingPreset) {
  return appliedPresets.some((applied) =>
    applied.presetId === preset.id || (isGamingMinPingPreset(applied) && isGamingMinPingPreset(preset))
  );
}

export default function Workshop() {
  const [tab, setTab] = useState<'browse' | 'rules'>('browse');
  const init = useWorkshopStore((s) => s.init);
  const appliedCount = useWorkshopStore((s) => s.appliedPresets.length);
  const customCount = useWorkshopStore((s) => s.myRules.length);
  const { t } = useTranslation();

  useEffect(() => {
    init();
  }, [init]);

  return (
    <div className="flex-1 p-5 overflow-y-auto animate-fade-in">
      <div className="max-w-3xl mx-auto space-y-4">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between mb-6">
          <h1 className="text-3xl font-black text-black flex items-center gap-4 drop-shadow-[2px_2px_0_#fff] tracking-tighter uppercase">
            <span className="p-3 bg-black text-white rounded-xl shadow-[4px_4px_0_#000] border-[3px] border-black">
              <Wrench className="w-6 h-6 stroke-[3px]" />
            </span>
            {t('workshop')}
          </h1>

          <div className="grid grid-cols-2 bg-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-xl p-1 gap-1">
            <button
              onClick={() => setTab('browse')}
              className={`flex items-center justify-center gap-2 px-4 py-2 rounded-lg text-[11px] font-black uppercase tracking-widest cursor-pointer transition-all duration-150 border-[2px] ${
                tab === 'browse'
                  ? 'bg-black text-white border-black shadow-[2px_2px_0_rgba(0,0,0,0.5)] translate-x-[-1px] translate-y-[-1px]'
                  : 'bg-transparent text-black border-transparent hover:bg-black/5'
              }`}
            >
              <Users className="w-4 h-4 stroke-[3px]" />
              Наборы
            </button>
            <button
              onClick={() => setTab('rules')}
              className={`flex items-center justify-center gap-2 px-4 py-2 rounded-lg text-[11px] font-black uppercase tracking-widest cursor-pointer transition-all duration-150 border-[2px] ${
                tab === 'rules'
                  ? 'bg-black text-white border-black shadow-[2px_2px_0_rgba(0,0,0,0.5)] translate-x-[-1px] translate-y-[-1px]'
                  : 'bg-transparent text-black border-transparent hover:bg-black/5'
              }`}
            >
              <Sparkles className="w-4 h-4 stroke-[3px]" />
              Мои
              {(appliedCount + customCount) > 0 && (
                <span className={`ml-0.5 rounded-full px-1.5 py-0.5 text-[9px] ${tab === 'rules' ? 'bg-white text-black' : 'bg-black text-white'}`}>
                  {appliedCount + customCount}
                </span>
              )}
            </button>
          </div>
        </div>

        {tab === 'browse' ? <BrowseTab onOpenRules={() => setTab('rules')} /> : <MyRulesTab onOpenBrowse={() => setTab('browse')} />}
      </div>
    </div>
  );
}

function BrowseTab({ onOpenRules }: { onOpenRules: () => void }) {
  const {
    presets,
    sortBy,
    setSortBy,
    applyPreset,
    appliedPresets,
    loading,
    loadPresets,
  } = useWorkshopStore();
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [appliedId, setAppliedId] = useState<string | null>(null);
  const { t } = useTranslation();

  const handleApply = (id: string) => {
    applyPreset(id);
    setAppliedId(id);
    setTimeout(() => setAppliedId(null), 1600);
  };

  const sorted = [...presets].sort((a, b) => {
    if (sortBy === 'newest') return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
    if (sortBy === 'top-rated') return b.stars - a.stars;
    return (b.upvotes + b.totalRatings + b.stars) - (a.upvotes + a.totalRatings + a.stars);
  });

  return (
    <>
      <div className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 className="text-xl font-black uppercase tracking-tight text-black">Готовые наборы</h2>
            <p className="text-xs font-bold text-black/55 mt-1 leading-relaxed">
              Работают в режиме «Весь компьютер». В обычном Proxy эти наборы просто сохраняются и не меняют трафик.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <div className="flex bg-white border-[3px] border-black rounded-xl p-1 gap-1 shadow-[2px_2px_0_#000]">
              {(['popular', 'newest', 'top-rated'] as const).map((sort) => (
                <button
                  key={sort}
                  onClick={() => setSortBy(sort)}
                  className={`px-3 py-1.5 text-[10px] rounded-lg font-black uppercase tracking-widest cursor-pointer transition-all border-[2px] ${
                    sortBy === sort
                      ? 'bg-black text-white border-black'
                      : 'bg-transparent text-black border-transparent hover:bg-black/5'
                  }`}
                >
                  {SORT_LABELS[sort]}
                </button>
              ))}
            </div>
            <button
              onClick={() => loadPresets()}
              className="flex items-center justify-center gap-2 px-3 py-2 bg-black text-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
            >
              <RefreshCw className="w-3.5 h-3.5 stroke-[3px]" />
              {t('retry')}
            </button>
          </div>
        </div>
      </div>

      {loading ? (
        <div className="text-center py-16">
          <p className="text-sm font-black text-black/40 uppercase tracking-widest animate-pulse">{t('loadingPresets')}</p>
        </div>
      ) : sorted.length === 0 ? (
        <div className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] p-8 text-center space-y-4">
          <p className="text-sm font-black text-black/50 uppercase tracking-widest">{t('noPresets')}</p>
          <p className="text-xs text-black/40 font-bold">{t('apiUnreachable')}</p>
          <button
            onClick={() => loadPresets()}
            className="px-5 py-2 bg-black text-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-xl text-xs font-black uppercase tracking-widest cursor-pointer hover:-translate-y-1 hover:shadow-[6px_6px_0_#000] active:translate-y-1 active:shadow-none transition-all"
          >
            {t('retry')}
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {sorted.map((preset) => (
            <PresetCard
              key={preset.id}
              preset={preset}
              isExpanded={expandedId === preset.id}
              isApplied={isPresetApplied(appliedPresets, preset)}
              justApplied={appliedId === preset.id}
              onToggle={() => setExpandedId(expandedId === preset.id ? null : preset.id)}
              onApply={() => handleApply(preset.id)}
            />
          ))}
        </div>
      )}

      <button
        onClick={onOpenRules}
        className="w-full py-4 text-xs font-black uppercase tracking-widest text-black/60 hover:text-black cursor-pointer border-[3px] border-dashed border-black/35 hover:border-black rounded-2xl text-center transition-all hover:bg-white"
      >
        Открыть мои правила и ручную настройку
      </button>
    </>
  );
}

function PresetCard({
  preset,
  isExpanded,
  isApplied,
  justApplied,
  onToggle,
  onApply,
}: {
  preset: RoutingPreset;
  isExpanded: boolean;
  isApplied: boolean;
  justApplied: boolean;
  onToggle: () => void;
  onApply: () => void;
}) {
  const counts = getActionCounts(preset.rules);
  const sampleRules = preset.rules.slice(0, 4);

  return (
    <div className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] overflow-hidden transition-all hover:-translate-y-0.5 hover:shadow-[8px_8px_0_#000]">
      <div className="p-5">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="text-xl font-black text-black uppercase tracking-tight leading-tight">{preset.title}</h3>
              {isApplied && (
                <span className="inline-flex items-center gap-1 shrink-0 rounded-lg border-2 border-black bg-emerald-400 px-2 py-1 text-[9px] font-black uppercase tracking-widest text-black">
                  <CheckCircle2 className="w-3 h-3 stroke-[3px]" />
                  Включено
                </span>
              )}
            </div>
            <p className="text-xs font-bold text-black/65 mt-1 leading-relaxed">{preset.description}</p>

            <div className="flex flex-wrap gap-2 mt-4">
              {sampleRules.map((rule) => (
                <span key={rule.id} className="inline-flex max-w-full items-center gap-1.5 text-[10px] px-2 py-1 rounded-lg font-black uppercase tracking-widest border-[2px] border-black bg-white text-black">
                  <RuleIcon type={rule.type} className="w-3 h-3 stroke-[3px] shrink-0" />
                  <span className="truncate">{rule.value}</span>
                </span>
              ))}
              {preset.rules.length > sampleRules.length && (
                <span className="text-[10px] font-black text-black/55 uppercase tracking-widest self-center">+{preset.rules.length - sampleRules.length}</span>
              )}
            </div>

            <div className="flex flex-wrap gap-2 mt-4">
              <ActionCount action="proxy" count={counts.proxy} />
              <ActionCount action="direct" count={counts.direct} />
              {counts.block > 0 && <ActionCount action="block" count={counts.block} />}
            </div>
          </div>

          <div className="grid grid-cols-[1fr_auto] gap-2 sm:flex sm:flex-col sm:w-40 shrink-0">
            <button
              onClick={onApply}
              disabled={isApplied}
              className={`px-4 py-3 text-sm border-[3px] rounded-xl font-black uppercase tracking-widest cursor-pointer shadow-[3px_3px_0_#000] hover:-translate-y-0.5 hover:shadow-[5px_5px_0_#000] active:translate-y-1 active:shadow-none transition-all disabled:cursor-default disabled:hover:translate-y-0 disabled:hover:shadow-[3px_3px_0_#000] ${
                isApplied || justApplied ? 'bg-emerald-400 text-black border-black' : 'bg-black text-white border-black'
              }`}
            >
              {isApplied || justApplied ? 'Включено' : 'Включить'}
            </button>
            <button
              onClick={onToggle}
              className="group flex items-center justify-center gap-2 text-black bg-white border-[3px] border-black rounded-xl cursor-pointer px-3 py-3 shadow-[2px_2px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
              title="Показать правила"
            >
              <SlidersHorizontal className="w-4 h-4 stroke-[3px]" />
              {isExpanded ? <ChevronUp className="w-4 h-4 stroke-[3px]" /> : <ChevronDown className="w-4 h-4 stroke-[3px]" />}
            </button>
          </div>
        </div>
      </div>

      {isExpanded && (
        <RulesGrid rules={preset.rules} className="px-5 pb-5 border-t-[3px] border-black bg-white/60 animate-slide-up" />
      )}
    </div>
  );
}

function MyRulesTab({ onOpenBrowse }: { onOpenBrowse: () => void }) {
  const {
    myRules,
    appliedPresets,
    addRule,
    removeRule,
    toggleRule,
    setRuleAction,
    removeAppliedPreset,
    toggleAppliedRule,
    setAppliedRuleAction,
    removeAppliedRule,
  } = useWorkshopStore();
  const [newType, setNewType] = useState<'domain' | 'exe'>('domain');
  const [newValue, setNewValue] = useState('');
  const [newComment, setNewComment] = useState('');
  const [newAction, setNewAction] = useState<RoutingRule['action']>('proxy');
  const [expandedPreset, setExpandedPreset] = useState<string | null>(null);
  const [showManual, setShowManual] = useState(false);

  const [showAppScanner, setShowAppScanner] = useState(false);
  const [installedApps, setInstalledApps] = useState<{ name: string; path: string }[]>([]);
  const [appSearch, setAppSearch] = useState('');
  const [scanningApps, setScanningApps] = useState(false);

  const allRules = [...appliedPresets.flatMap((ap) => ap.rules), ...myRules];
  const counts = getActionCounts(allRules.filter((rule) => rule.enabled));

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

  const handleScanApps = useCallback(async () => {
    setShowAppScanner(true);
    setScanningApps(true);
    try {
      const apps = await desktopBridge.command<{ name: string; path: string }[]>('scan_installed_apps');
      setInstalledApps(apps);
    } catch {
      setInstalledApps([]);
    } finally {
      setScanningApps(false);
    }
  }, []);

  const handleSelectApp = useCallback((app: { name: string; path: string }) => {
    setNewValue(app.path.toLowerCase());
    setNewType('exe');
    setNewComment(app.name);
    setShowAppScanner(false);
    setShowManual(true);
  }, []);

  const handleExport = useCallback(() => {
    const allExportRules = [...myRules, ...appliedPresets.flatMap((ap) => ap.rules)];
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
    input.onchange = async (event) => {
      const file = (event.target as HTMLInputElement).files?.[0];
      if (!file) return;
      try {
        if (file.size > 1024 * 1024) throw new Error('File is too large');
        const text = await file.text();
        const rules = JSON.parse(text);
        if (!Array.isArray(rules) || rules.length > 256) throw new Error('Invalid format');
        for (const rule of rules) {
          if (!rule || typeof rule !== 'object') throw new Error('Invalid rule');
          if (rule.type !== 'domain' && rule.type !== 'exe') throw new Error('Invalid rule type');
          if (rule.action !== 'proxy' && rule.action !== 'direct' && rule.action !== 'block') throw new Error('Invalid rule action');
          if (typeof rule.value !== 'string' || !rule.value.trim() || rule.value.length > 2048) throw new Error('Invalid rule value');
          if (rule.comment !== undefined && (typeof rule.comment !== 'string' || rule.comment.length > 500)) throw new Error('Invalid rule comment');
          addRule({
            id: crypto.randomUUID(),
            type: rule.type,
            value: rule.value.trim(),
            action: rule.action,
            enabled: rule.enabled !== false,
            comment: rule.comment?.trim() || undefined,
          });
        }
      } catch {
        alert('Invalid rules file');
      }
    };
    input.click();
  }, [addRule]);

  const filteredApps = installedApps.filter((app) => {
    const query = appSearch.toLowerCase();
    return app.name.toLowerCase().includes(query) || app.path.toLowerCase().includes(query);
  });

  return (
    <>
      {allRules.length > 0 ? (
        <div className="grid grid-cols-3 gap-2">
          <Stat label="VPN" count={counts.proxy} />
          <Stat label="Прямо" count={counts.direct} />
          <Stat label="Блок" count={counts.block} />
        </div>
      ) : (
        <div className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] p-6 text-center space-y-4">
          <p className="text-sm font-black uppercase tracking-widest text-black/55">Правила пока не включены</p>
          <button
            onClick={onOpenBrowse}
            className="inline-flex items-center justify-center gap-2 px-5 py-3 bg-black text-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-xl text-xs font-black uppercase tracking-widest cursor-pointer hover:-translate-y-1 hover:shadow-[6px_6px_0_#000] active:translate-y-1 active:shadow-none transition-all"
          >
            <Users className="w-4 h-4 stroke-[3px]" />
            Выбрать набор
          </button>
        </div>
      )}

      {appliedPresets.length > 0 && (
        <div className="space-y-3">
          <SectionLabel>Включенные наборы</SectionLabel>
          {appliedPresets.map((preset) => {
            const isExpanded = expandedPreset === preset.presetId;
            const activeCount = preset.rules.filter((rule) => rule.enabled).length;
            return (
              <div key={preset.presetId} className="bg-white border-[4px] border-black shadow-[6px_6px_0_#000] rounded-2xl overflow-hidden transition-all hover:shadow-[8px_8px_0_#000]">
                <div className="px-5 py-4 flex items-center gap-3">
                  <CheckCircle2 className="w-6 h-6 stroke-[3px] text-emerald-600 shrink-0" />
                  <div className="flex-1 min-w-0">
                    <h3 className="text-sm font-black text-black uppercase tracking-tight truncate">{preset.title}</h3>
                    <p className="text-[10px] font-bold text-black/50 uppercase tracking-widest mt-0.5 truncate">
                      {activeCount} из {preset.rules.length} активны
                    </p>
                  </div>
                  <button
                    onClick={() => setExpandedPreset(isExpanded ? null : preset.presetId)}
                    className="group flex items-center justify-center text-black bg-white border-[3px] border-black rounded-xl cursor-pointer p-2 shadow-[2px_2px_0_#000] hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
                    title="Показать правила"
                  >
                    {isExpanded ? <ChevronUp className="w-5 h-5 stroke-[3px]" /> : <ChevronDown className="w-5 h-5 stroke-[3px]" />}
                  </button>
                  <button
                    onClick={() => removeAppliedPreset(preset.presetId)}
                    className="group p-2 rounded-xl cursor-pointer border-[3px] border-black shadow-[2px_2px_0_#000] active:translate-x-1 active:translate-y-1 active:shadow-none bg-black text-white hover:bg-danger transition-all"
                    title="Удалить набор"
                  >
                    <Trash2 className="w-4 h-4 stroke-[3px]" />
                  </button>
                </div>

                {isExpanded && (
                  <div className="px-5 pb-5 border-t-[3px] border-black bg-white/60 animate-slide-up">
                    <div className="space-y-2 mt-3">
                      {preset.rules.map((rule) => (
                        <EditableRuleRow
                          key={rule.id}
                          rule={rule}
                          onToggle={() => toggleAppliedRule(preset.presetId, rule.id)}
                          onRemove={() => removeAppliedRule(preset.presetId, rule.id)}
                          onSetAction={(action) => setAppliedRuleAction(preset.presetId, rule.id, action)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      <div className="bg-white border-[4px] border-black rounded-2xl shadow-[6px_6px_0_#000] overflow-hidden">
        <button
          onClick={() => setShowManual(!showManual)}
          className="w-full flex items-center justify-between gap-4 px-5 py-4 text-left cursor-pointer hover:bg-black/5 transition-colors"
        >
          <span className="flex items-center gap-3">
            <SlidersHorizontal className="w-5 h-5 stroke-[3px]" />
            <span>
              <span className="block text-sm font-black uppercase tracking-widest text-black">Ручная настройка</span>
              <span className="block text-[10px] font-bold uppercase tracking-widest text-black/45 mt-0.5">Домены, приложения, импорт и экспорт</span>
            </span>
          </span>
          {showManual ? <ChevronUp className="w-5 h-5 stroke-[3px]" /> : <ChevronDown className="w-5 h-5 stroke-[3px]" />}
        </button>

        {showManual && (
          <div className="border-t-[3px] border-black bg-bg-primary p-5 space-y-4 animate-slide-up">
            <div className="flex flex-col sm:flex-row gap-3">
              <div className="grid grid-cols-2 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl p-1 gap-1 shrink-0">
                <button
                  onClick={() => setNewType('domain')}
                  className={`flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all border-[2px] ${
                    newType === 'domain' ? 'bg-black text-white border-black' : 'text-black border-transparent hover:bg-black/5'
                  }`}
                >
                  <Globe className="w-4 h-4 stroke-[3px]" />
                  Сайт
                </button>
                <button
                  onClick={() => setNewType('exe')}
                  className={`flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-xs font-black uppercase tracking-widest cursor-pointer transition-all border-[2px] ${
                    newType === 'exe' ? 'bg-black text-white border-black' : 'text-black border-transparent hover:bg-black/5'
                  }`}
                >
                  <FileCode className="w-4 h-4 stroke-[3px]" />
                  Приложение
                </button>
              </div>
              <input
                type="text"
                value={newValue}
                onChange={(event) => setNewValue(event.target.value)}
                onKeyDown={(event) => event.key === 'Enter' && handleAdd()}
                placeholder={newType === 'domain' ? 'youtube.com' : 'Discord.exe'}
                className="flex-1 w-full min-w-0 bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-2 text-sm text-black placeholder:text-black/50 focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow font-black tracking-tight"
              />
            </div>
            <p className="text-[10px] font-bold uppercase tracking-widest text-black/45">
              {newType === 'exe'
                ? 'Приложения можно направлять только в режиме «Весь компьютер».'
                : 'Сайты можно направлять только в режиме «Весь компьютер». В Proxy эти правила просто сохраняются.'}
            </p>

            <div className="flex flex-col sm:flex-row gap-3">
              <input
                type="text"
                value={newComment}
                onChange={(event) => setNewComment(event.target.value)}
                onKeyDown={(event) => event.key === 'Enter' && handleAdd()}
                placeholder="Заметка"
                className="flex-1 w-full min-w-0 bg-white border-[3px] border-black shadow-inner rounded-xl px-4 py-2 text-sm text-black placeholder:text-black/50 focus:outline-none focus:shadow-[2px_2px_0_#000] transition-shadow font-black tracking-tight"
              />
              <div className="flex flex-wrap sm:flex-nowrap gap-3 shrink-0">
                <ActionPicker value={newAction} onChange={setNewAction} />
                {newType === 'exe' && (
                  <button
                    onClick={handleScanApps}
                    className="group flex items-center justify-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-xs font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
                  >
                    <Monitor className="w-4 h-4 stroke-[3px]" />
                    Найти
                  </button>
                )}
                <button
                  onClick={handleAdd}
                  disabled={!newValue.trim()}
                  className="group px-5 py-2 bg-black text-white border-[3px] border-black shadow-[4px_4px_0_#000] active:translate-x-[4px] active:translate-y-[4px] active:shadow-none hover:-translate-y-1 hover:-translate-x-1 hover:shadow-[6px_6px_0_#000] rounded-xl text-sm font-black cursor-pointer transition-all disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center"
                >
                  <Plus className="w-5 h-5 stroke-[4px]" />
                  Добавить
                </button>
              </div>
            </div>

            <div className="flex flex-wrap gap-2 pt-1">
              <button
                onClick={handleExport}
                className="group flex items-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
              >
                <Download className="w-3.5 h-3.5 stroke-[3px]" />
                Экспорт
              </button>
              <button
                onClick={handleImport}
                className="group flex items-center gap-2 px-4 py-2 bg-white text-black border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:-translate-y-0.5 hover:shadow-[4px_4px_0_#000] active:translate-y-0.5 active:shadow-none transition-all"
              >
                <Upload className="w-3.5 h-3.5 stroke-[3px]" />
                Импорт
              </button>
            </div>
          </div>
        )}
      </div>

      {myRules.length > 0 && (
        <div className="space-y-3">
          <SectionLabel>Добавлено вручную</SectionLabel>
          {myRules.map((rule) => (
            <EditableRuleRow
              key={rule.id}
              rule={rule}
              onToggle={() => toggleRule(rule.id)}
              onRemove={() => removeRule(rule.id)}
              onSetAction={(action) => setRuleAction(rule.id, action)}
            />
          ))}
        </div>
      )}

      {showAppScanner && (
        <AppScannerModal
          apps={filteredApps}
          search={appSearch}
          scanning={scanningApps}
          onSearch={setAppSearch}
          onClose={() => setShowAppScanner(false)}
          onSelect={handleSelectApp}
        />
      )}
    </>
  );
}

function EditableRuleRow({
  rule,
  onToggle,
  onRemove,
  onSetAction,
}: {
  rule: RoutingRule;
  onToggle: () => void;
  onRemove: () => void;
  onSetAction: (action: RoutingRule['action']) => void;
}) {
  return (
    <div className={`group bg-white border-[3px] border-black shadow-[4px_4px_0_#000] rounded-2xl px-4 py-3 flex flex-col gap-3 sm:flex-row sm:items-center transition-all ${!rule.enabled ? 'opacity-50 grayscale' : ''}`}>
      <div className="flex items-center gap-3 flex-1 min-w-0">
        <div className="w-10 h-10 rounded-xl border-[3px] border-black bg-white flex items-center justify-center shrink-0">
          <RuleIcon type={rule.type} className="w-5 h-5 stroke-[3px]" />
        </div>
        <div className="flex-1 min-w-0">
          <p className={`text-sm font-black tracking-tight truncate transition-colors ${rule.enabled ? 'text-black' : 'text-black/60'}`}>
            {rule.value}
          </p>
          <p className="text-[10px] font-black tracking-widest text-black/45 uppercase mt-1 truncate">
            {rule.comment || (rule.type === 'exe' ? 'Приложение' : 'Сайт')}
          </p>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2 sm:justify-end">
        <ActionPicker value={rule.action} onChange={onSetAction} compact />
        <button
          onClick={onToggle}
          className={`px-3 py-2 rounded-xl border-[3px] border-black text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[2px_2px_0_#000] transition-all ${
            rule.enabled ? 'bg-emerald-400 text-black' : 'bg-white text-black'
          }`}
        >
          {rule.enabled ? 'Вкл' : 'Выкл'}
        </button>
        <button
          onClick={onRemove}
          className="p-2 rounded-xl transition-all cursor-pointer border-[3px] border-black shadow-[2px_2px_0_#000] active:translate-x-1 active:translate-y-1 active:shadow-none bg-black text-white hover:bg-danger"
          title="Удалить"
        >
          <Trash2 className="w-4 h-4 stroke-[3px]" />
        </button>
      </div>
    </div>
  );
}

function RulesGrid({ rules, className = '' }: { rules: RoutingRule[]; className?: string }) {
  return (
    <div className={className}>
      <div className="grid sm:grid-cols-2 gap-2 mt-4">
        {rules.map((rule) => (
          <div key={rule.id} className="flex items-center gap-3 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl px-4 py-2">
            <RuleIcon type={rule.type} className="w-4 h-4 stroke-[3px] shrink-0" />
            <div className="flex-1 min-w-0">
              <span className="text-xs text-black font-black tracking-tight block truncate">{rule.value}</span>
              {rule.comment && <span className="text-[9px] text-black/50 font-black tracking-widest lowercase block truncate">{rule.comment}</span>}
            </div>
            <ActionBadge action={rule.action} />
          </div>
        ))}
      </div>
    </div>
  );
}

function ActionPicker({
  value,
  onChange,
  compact = false,
}: {
  value: RoutingRule['action'];
  onChange: (action: RoutingRule['action']) => void;
  compact?: boolean;
}) {
  return (
    <div className="flex gap-1 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl p-1 shrink-0">
      {ACTION_ORDER.map((action) => {
        const config = ACTION_CONFIG[action];
        const Icon = config.icon;
        const isActive = value === action;
        return (
          <button
            key={action}
            onClick={() => onChange(action)}
            className={`flex items-center justify-center gap-1 rounded-lg text-[10px] font-black uppercase tracking-widest cursor-pointer transition-all border-[2px] ${
              compact ? 'px-2 py-1.5' : 'px-3 py-1.5'
            } ${isActive ? `${config.bg} ${config.text}` : 'text-black/55 border-transparent hover:bg-black/5'}`}
            title={config.label}
          >
            <Icon className="w-3.5 h-3.5 stroke-[3px]" />
            {isActive && <span>{compact ? config.shortLabel : config.label}</span>}
          </button>
        );
      })}
    </div>
  );
}

function ActionBadge({ action }: { action: RoutingRule['action'] }) {
  const config = ACTION_CONFIG[action];
  const Icon = config.icon;

  return (
    <span className={`inline-flex items-center gap-1 text-[9px] px-2 py-1 rounded-lg font-black uppercase tracking-widest border-2 ${config.bg} ${config.text}`}>
      <Icon className="w-3 h-3 stroke-[3px]" />
      {config.shortLabel}
    </span>
  );
}

function ActionCount({ action, count }: { action: RoutingRule['action']; count: number }) {
  const config = ACTION_CONFIG[action];
  const Icon = config.icon;

  return (
    <span className="inline-flex items-center gap-1.5 text-[10px] font-black uppercase tracking-widest text-black/70 bg-black/5 px-2 py-1 rounded-lg border-2 border-black/10">
      <Icon className="w-3.5 h-3.5 stroke-[3px]" />
      {config.shortLabel}: {count}
    </span>
  );
}

function RuleIcon({ type, className }: { type: RoutingRule['type']; className?: string }) {
  const Icon = type === 'exe' ? FileCode : Globe;
  return <Icon className={className} />;
}

function SectionLabel({ children }: { children: string }) {
  return <p className="text-[10px] font-black uppercase tracking-widest text-black/50 px-1">{children}</p>;
}

function Stat({ label, count }: { label: string; count: number }) {
  return (
    <div className="bg-bg-primary border-[4px] border-black rounded-2xl px-4 py-3 text-center shadow-[4px_4px_0_#000]">
      <p className="text-2xl font-black text-black drop-shadow-[2px_2px_0_#fff]">{count}</p>
      <p className="text-[10px] text-black/60 font-black uppercase tracking-widest mt-1">{label}</p>
    </div>
  );
}

function AppScannerModal({
  apps,
  search,
  scanning,
  onSearch,
  onClose,
  onSelect,
}: {
  apps: { name: string; path: string }[];
  search: string;
  scanning: boolean;
  onSearch: (value: string) => void;
  onClose: () => void;
  onSelect: (app: { name: string; path: string }) => void;
}) {
  return (
    <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4" onClick={onClose}>
      <div className="bg-white border-[4px] border-black rounded-2xl shadow-[8px_8px_0_#000] w-full max-w-lg max-h-[70vh] flex flex-col animate-slide-up" onClick={(event) => event.stopPropagation()}>
        <div className="flex items-center gap-3 p-5 border-b-[3px] border-black">
          <Monitor className="w-5 h-5 stroke-[3px]" />
          <h3 className="text-lg font-black uppercase tracking-tight">Установленные приложения</h3>
          <button onClick={onClose} className="ml-auto text-xl font-black cursor-pointer px-2">x</button>
        </div>
        <div className="px-5 pt-4 pb-2">
          <div className="flex items-center gap-2 bg-white border-[3px] border-black shadow-inner rounded-xl px-3 py-2">
            <Search className="w-4 h-4 stroke-[3px] text-black/40" />
            <input
              type="text"
              value={search}
              onChange={(event) => onSearch(event.target.value)}
              placeholder="Поиск"
              autoFocus
              className="flex-1 text-sm font-black text-black bg-transparent placeholder:text-black/40 focus:outline-none tracking-tight"
            />
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-5 pb-5 space-y-1">
          {scanning ? (
            <p className="text-center py-8 text-sm font-black text-black/40 uppercase tracking-widest animate-pulse">Сканирование...</p>
          ) : apps.length === 0 ? (
            <p className="text-center py-8 text-sm font-black text-black/40 uppercase tracking-widest">Не найдено</p>
          ) : (
            apps.map((app, index) => (
              <button
                key={`${app.path}-${index}`}
                onClick={() => onSelect(app)}
                className="w-full flex items-center gap-3 px-4 py-3 bg-white border-[2px] border-black/20 rounded-xl hover:bg-black/5 hover:border-black transition-all cursor-pointer text-left"
              >
                <FileCode className="w-5 h-5 stroke-[3px] shrink-0" />
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
  );
}

function getActionCounts(rules: RoutingRule[]) {
  return rules.reduce(
    (counts, rule) => {
      counts[rule.action] += 1;
      return counts;
    },
    { proxy: 0, direct: 0, block: 0 } satisfies Record<RoutingRule['action'], number>,
  );
}
