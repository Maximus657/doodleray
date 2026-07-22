import { useEffect, useState } from 'react';
import { X, Plus, Trash2, ShieldCheck, AlertTriangle, AppWindow, ArrowLeft, Check, Loader2, Search } from 'lucide-react';
import { useWorkshopStore, isGamingMinPingPreset, type RoutingRule } from '../../stores/workshop-store';
import Toggle from './Toggle';

type T = (key: never) => string;

const ACTION_COLORS: Record<RoutingRule['action'], string> = {
  direct: '#3ddc84',
  proxy: '#FF9E38',
  block: '#ff6b5a',
};

function actionKey(action: RoutingRule['action']): string {
  return action === 'direct' ? 'v6ActionDirect' : action === 'proxy' ? 'v6ActionProxy' : 'v6ActionBlock';
}

/** Normalize user input into a rule value: URLs → hostname; *.exe stays exe. */
function parseRuleInput(raw: string): { type: RoutingRule['type']; value: string } | null {
  let v = raw.trim().toLowerCase();
  if (!v) return null;
  if (v.endsWith('.exe')) return { type: 'exe', value: v.replace(/^.*[\\/]/, '') };
  v = v.replace(/^[a-z]+:\/\//, '').replace(/[/?#].*$/, '').replace(/^www\./, '');
  if (!/^[a-z0-9.-]+\.[a-z]{2,}$/.test(v)) return null;
  return { type: 'domain', value: v };
}

interface RunningApp {
  name: string;
  path: string;
}

interface PickerState {
  apps: RunningApp[] | null;
  selected?: RunningApp;
  exes?: string[];
  checked: Set<string>;
  search: string;
}

interface Props {
  protectedMode: boolean;
  onClose: () => void;
  t: T;
}

/**
 * Simple v6 split-routing editor over the real Workshop store: one input to
 * add a rule, an app picker (running apps → their sibling .exe files, since
 * games often need several executables routed together), per-rule
 * toggle/delete, and the gaming preset as a one-tap apply.
 */
export default function SplitRoutingModal({ protectedMode, onClose, t }: Props) {
  const {
    myRules, addRule, removeRule, toggleRule,
    appliedPresets, removeAppliedPreset,
    presets, applyPreset, loadPresets,
  } = useWorkshopStore();

  const [input, setInput] = useState('');
  const [action, setAction] = useState<RoutingRule['action']>('direct');
  const [invalid, setInvalid] = useState(false);
  const [picker, setPicker] = useState<PickerState | null>(null);

  useEffect(() => {
    if (presets.length === 0) loadPresets().catch(() => {});
  }, [presets.length, loadPresets]);

  const handleAdd = () => {
    const parsed = parseRuleInput(input);
    if (!parsed) { setInvalid(true); return; }
    setInvalid(false);
    addRule({ id: crypto.randomUUID(), ...parsed, action, enabled: true });
    setInput('');
  };

  const openPicker = async () => {
    setPicker({ apps: null, checked: new Set(), search: '' });
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const apps = await invoke<RunningApp[]>('list_running_apps');
      setPicker((p) => (p ? { ...p, apps } : p));
    } catch {
      setPicker((p) => (p ? { ...p, apps: [] } : p));
    }
  };

  const pickApp = async (app: RunningApp) => {
    const main = app.path.replace(/^.*[\\/]/, '');
    setPicker((p) => (p ? { ...p, selected: app, exes: undefined, checked: new Set([main]) } : p));
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const exes = await invoke<string[]>('list_dir_exes', { exePath: app.path });
      setPicker((p) => (p && p.selected?.path === app.path ? { ...p, exes: exes.length > 0 ? exes : [main] } : p));
    } catch {
      setPicker((p) => (p && p.selected?.path === app.path ? { ...p, exes: [main] } : p));
    }
  };

  const addCheckedExes = () => {
    if (!picker) return;
    const existing = new Set(myRules.map((r) => `${r.type}:${r.value.toLowerCase()}`));
    for (const exe of picker.checked) {
      if (existing.has(`exe:${exe.toLowerCase()}`)) continue;
      addRule({ id: crypto.randomUUID(), type: 'exe', value: exe, action, enabled: true });
    }
    setPicker(null);
  };

  // Catalog is intentionally trimmed to the gaming preset — one set with a
  // clear honest job: games direct for min ping, everything else via VPN.
  const gamingPreset = presets.find(
    (p) => isGamingMinPingPreset(p) && !appliedPresets.some((ap) => ap.presetId === p.id || ap.title === p.title || isGamingMinPingPreset(ap)),
  );

  const filteredApps = picker?.apps?.filter(
    (a) => !picker.search || a.name.toLowerCase().includes(picker.search.toLowerCase()),
  );

  return (
    <div
      onClick={onClose}
      className="v6-fadein absolute inset-0 z-20 flex items-center justify-center"
      style={{ background: 'rgba(10,5,8,0.5)', backdropFilter: 'blur(8px)', WebkitBackdropFilter: 'blur(8px)' }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="v6-modal flex max-h-[calc(100vh-88px)] w-[min(480px,calc(100vw-48px))] flex-col rounded-[28px] p-[26px] pt-[22px]"
      >
        <div className="mb-1.5 flex shrink-0 items-center justify-between">
          <span className="flex items-center gap-2 text-[18px] font-semibold text-white">
            {picker && (
              <button
                type="button"
                onClick={() => setPicker(picker.selected ? { ...picker, selected: undefined, exes: undefined, checked: new Set() } : null)}
                aria-label={t('v6Back' as never)}
                className="v6-hover-bright flex h-[30px] w-[30px] items-center justify-center rounded-[10px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
              >
                <ArrowLeft className="h-4 w-4" strokeWidth={2.2} />
              </button>
            )}
            {picker ? t('v6PickApp' as never) : t('v6SplitTitle' as never)}
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('cancel' as never)}
            className="v6-hover-bright flex h-[34px] w-[34px] items-center justify-center rounded-[11px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
          >
            <X className="h-4 w-4" strokeWidth={2.3} />
          </button>
        </div>

        {/* ── App picker mode ── */}
        {picker ? (
          <div className="flex min-h-0 flex-1 flex-col">
            {!picker.selected ? (
              <>
                <div className="mb-3 text-[12px] leading-snug text-white/50">{t('v6PickAppHint' as never)}</div>
                <div className="v6-glass-inset mb-2.5 flex h-[40px] shrink-0 items-center gap-2 rounded-[13px] px-3">
                  <Search className="h-4 w-4 shrink-0 text-white/50" strokeWidth={2} />
                  <input
                    type="text"
                    value={picker.search}
                    onChange={(e) => setPicker({ ...picker, search: e.target.value })}
                    placeholder={t('search' as never)}
                    className="min-w-0 flex-1 bg-transparent text-[13px] text-white outline-none placeholder:text-white/40"
                  />
                </div>
                <div className="-mr-3 min-h-0 flex-1 overflow-y-auto pr-3">
                  {picker.apps === null ? (
                    <div className="grid place-items-center py-10"><Loader2 className="h-5 w-5 v6-orb-spin text-white/50" /></div>
                  ) : filteredApps && filteredApps.length > 0 ? (
                    filteredApps.map((app) => (
                      <button
                        key={app.path}
                        type="button"
                        onClick={() => pickApp(app)}
                        className="flex w-full items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5 text-left v6-focus"
                      >
                        <span className="v6-tile-accent flex h-8 w-8 shrink-0 items-center justify-center rounded-[10px]">
                          <AppWindow className="h-4 w-4" strokeWidth={1.9} />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[13.5px] font-medium text-white">{app.name}</span>
                          <span className="block truncate text-[10.5px] text-white/35" title={app.path}>{app.path}</span>
                        </span>
                      </button>
                    ))
                  ) : (
                    <div className="py-10 text-center text-[12px] text-white/40">{t('v6NoResults' as never)}</div>
                  )}
                </div>
              </>
            ) : (
              <>
                <div className="mb-3 text-[12px] leading-snug text-white/50">{t('v6RelatedExes' as never)}</div>
                <div className="-mr-3 min-h-0 flex-1 overflow-y-auto pr-3">
                  {!picker.exes ? (
                    <div className="grid place-items-center py-10"><Loader2 className="h-5 w-5 v6-orb-spin text-white/50" /></div>
                  ) : (
                    picker.exes.map((exe) => {
                      const on = picker.checked.has(exe);
                      return (
                        <button
                          key={exe}
                          type="button"
                          onClick={() => {
                            const checked = new Set(picker.checked);
                            if (on) checked.delete(exe); else checked.add(exe);
                            setPicker({ ...picker, checked });
                          }}
                          className="flex w-full items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5 text-left v6-focus"
                        >
                          <span
                            className="grid h-5 w-5 shrink-0 place-items-center rounded-[6px] border transition-colors"
                            style={{
                              background: on ? '#F97F16' : 'rgba(255,255,255,0.06)',
                              borderColor: on ? '#F97F16' : 'rgba(255,255,255,0.2)',
                            }}
                          >
                            {on && <Check className="h-3.5 w-3.5 text-white" strokeWidth={3} />}
                          </span>
                          <span className="min-w-0 flex-1 truncate text-[13.5px] font-medium text-white">{exe}</span>
                        </button>
                      );
                    })
                  )}
                </div>
                <button
                  type="button"
                  onClick={addCheckedExes}
                  disabled={picker.checked.size === 0}
                  className="mt-3 flex w-full shrink-0 items-center justify-center gap-2 rounded-[14px] py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40 v6-focus"
                  style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)', boxShadow: '0 6px 18px rgba(234,109,6,0.35)' }}
                >
                  <Plus className="h-4 w-4" strokeWidth={2.6} /> {t('add' as never)} ({picker.checked.size})
                </button>
              </>
            )}
          </div>
        ) : (
          <>
            <div className="mb-4 flex items-start gap-1.5 text-[12px] leading-snug text-white/50">
              {protectedMode
                ? <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[#3ddc84]" strokeWidth={2.2} />
                : <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-[#ffb02e]" strokeWidth={2.2} />}
              {protectedMode ? t('v6SplitModalHint' as never) : t('splitTunnelingNeedsTun' as never)}
            </div>

            {/* Add rule */}
            <div className="mb-1 flex shrink-0 gap-2">
              <input
                type="text"
                value={input}
                onChange={(e) => { setInput(e.target.value); setInvalid(false); }}
                onKeyDown={(e) => { if (e.key === 'Enter') handleAdd(); }}
                placeholder={t('v6RulePlaceholder' as never)}
                className={`v6-glass-inset min-w-0 flex-1 rounded-[13px] px-3.5 py-2.5 text-[13px] text-white outline-none placeholder:text-white/40 v6-focus ${invalid ? 'border-[#ff6b5a]/60' : ''}`}
              />
              <select
                value={action}
                onChange={(e) => setAction(e.target.value as RoutingRule['action'])}
                aria-label={t((action === 'direct' ? 'v6ActionDirect' : action === 'proxy' ? 'v6ActionProxy' : 'v6ActionBlock') as never)}
                className="cursor-pointer rounded-[13px] border border-white/[0.14] bg-white/[0.08] px-2.5 py-2 text-[12.5px] font-medium text-white outline-none v6-focus [&>option]:bg-[#1c1116]"
              >
                <option value="direct">{t('v6ActionDirect' as never)}</option>
                <option value="proxy">{t('v6ActionProxy' as never)}</option>
                <option value="block">{t('v6ActionBlock' as never)}</option>
              </select>
              <button
                type="button"
                onClick={openPicker}
                title={t('v6PickApp' as never)}
                aria-label={t('v6PickApp' as never)}
                className="v6-hover-bright grid h-[42px] w-[42px] shrink-0 place-items-center rounded-[13px] border border-white/[0.14] bg-white/[0.08] text-white/75 v6-focus"
              >
                <AppWindow className="h-[18px] w-[18px]" strokeWidth={2} />
              </button>
              <button
                type="button"
                onClick={handleAdd}
                aria-label={t('add' as never)}
                className="grid h-[42px] w-[42px] shrink-0 place-items-center rounded-[13px] text-white transition-opacity hover:opacity-90 v6-focus"
                style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)', boxShadow: '0 5px 14px rgba(234,109,6,0.35)' }}
              >
                <Plus className="h-[18px] w-[18px]" strokeWidth={2.6} />
              </button>
            </div>
            {invalid && <div className="mb-1 px-1 text-[11px] text-[#ffb3a8]">{t('v6RuleInvalid' as never)}</div>}

            <div className="-mr-3 min-h-0 flex-1 overflow-y-auto pr-3">
              {/* My rules */}
              {myRules.length > 0 && (
                <>
                  <div className="px-1 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{t('v6MyRules' as never)}</div>
                  {myRules.map((r) => (
                    <div key={r.id} className="flex items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5">
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13.5px] font-medium text-white" title={r.value}>{r.value}</span>
                      </span>
                      <span
                        className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                        style={{ color: ACTION_COLORS[r.action], background: `${ACTION_COLORS[r.action]}1a` }}
                      >
                        {t(actionKey(r.action) as never)}
                      </span>
                      <button type="button" onClick={() => toggleRule(r.id)} className="v6-focus" aria-label={r.value} aria-pressed={r.enabled}>
                        <Toggle on={r.enabled} label={r.value} />
                      </button>
                      <button
                        type="button"
                        onClick={() => removeRule(r.id)}
                        aria-label={`${t('cancel' as never)} ${r.value}`}
                        className="v6-hover-bright grid h-8 w-8 shrink-0 place-items-center rounded-[10px] border border-white/[0.1] bg-white/[0.06] text-white/60 v6-focus"
                      >
                        <Trash2 className="h-3.5 w-3.5" strokeWidth={2.2} />
                      </button>
                    </div>
                  ))}
                </>
              )}

              {/* Applied presets: one simple row each */}
              {appliedPresets.length > 0 && (
                <>
                  <div className="px-1 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{t('v6AppliedPresets' as never)}</div>
                  {appliedPresets.map((ap) => (
                    <div key={ap.presetId} className="flex items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5">
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13.5px] font-medium text-white" title={ap.title}>{ap.title}</span>
                        <span className="block text-[11px] text-white/40">
                          {ap.rules.filter((r) => r.enabled).length} {t('v6ActiveRules' as never)}
                        </span>
                      </span>
                      <button
                        type="button"
                        onClick={() => removeAppliedPreset(ap.presetId)}
                        aria-label={`${t('cancel' as never)} ${ap.title}`}
                        className="v6-hover-bright grid h-8 w-8 shrink-0 place-items-center rounded-[10px] border border-white/[0.1] bg-white/[0.06] text-white/60 v6-focus"
                      >
                        <Trash2 className="h-3.5 w-3.5" strokeWidth={2.2} />
                      </button>
                    </div>
                  ))}
                </>
              )}

              {/* Gaming preset: one-tap apply */}
              {gamingPreset && (
                <>
                  <div className="px-1 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{t('v6AvailablePresets' as never)}</div>
                  <div className="flex items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5">
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[13.5px] font-medium text-white" title={gamingPreset.title}>{gamingPreset.title}</span>
                      <span className="line-clamp-2 text-[11px] text-white/40" title={gamingPreset.description}>{gamingPreset.description}</span>
                    </span>
                    <button
                      type="button"
                      onClick={() => applyPreset(gamingPreset.id)}
                      className="shrink-0 rounded-[11px] px-3 py-1.5 text-[12px] font-semibold text-white transition-opacity hover:opacity-90 v6-focus"
                      style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)' }}
                    >
                      {t('v6Apply' as never)}
                    </button>
                  </div>
                </>
              )}

              {myRules.length === 0 && appliedPresets.length === 0 && !gamingPreset && (
                <div className="py-8 text-center text-[12px] text-white/40">{t('v6NoResults' as never)}</div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
