import { useEffect, useState } from 'react';
import { X, Plus, Trash2, ShieldCheck, AlertTriangle } from 'lucide-react';
import { useWorkshopStore, type RoutingRule } from '../../stores/workshop-store';
import Toggle from './Toggle';

type T = (key: never) => string;

const ACTION_COLORS: Record<RoutingRule['action'], string> = {
  direct: '#3ddc84',
  proxy: '#FF8A4C',
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

interface Props {
  protectedMode: boolean;
  onClose: () => void;
  t: T;
}

/**
 * Simple v6 split-routing editor over the real Workshop store: one input to
 * add a rule, toggle/delete per rule, applied presets as single rows, and
 * one-tap apply for catalog presets. Honest hint that rules need TUN mode.
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

  const availablePresets = presets.filter(
    (p) => !appliedPresets.some((ap) => ap.presetId === p.id || ap.title === p.title),
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
          <span className="text-[18px] font-semibold text-white">{t('v6SplitTitle' as never)}</span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('cancel' as never)}
            className="v6-hover-bright flex h-[34px] w-[34px] items-center justify-center rounded-[11px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
          >
            <X className="h-4 w-4" strokeWidth={2.3} />
          </button>
        </div>

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
            aria-label={t('v6ActionDirect' as never)}
            className="cursor-pointer rounded-[13px] border border-white/[0.14] bg-white/[0.08] px-2.5 py-2 text-[12.5px] font-medium text-white outline-none v6-focus [&>option]:bg-[#1c1116]"
          >
            <option value="direct">{t('v6ActionDirect' as never)}</option>
            <option value="proxy">{t('v6ActionProxy' as never)}</option>
            <option value="block">{t('v6ActionBlock' as never)}</option>
          </select>
          <button
            type="button"
            onClick={handleAdd}
            aria-label={t('add' as never)}
            className="grid h-[42px] w-[42px] shrink-0 place-items-center rounded-[13px] text-white transition-opacity hover:opacity-90 v6-focus"
            style={{ background: 'linear-gradient(140deg, #FF8A4C, #FF5A1F)', boxShadow: '0 5px 14px rgba(255,90,31,0.35)' }}
          >
            <Plus className="h-4.5 w-4.5" strokeWidth={2.6} />
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
                  <button type="button" onClick={() => toggleRule(r.id)} className="v6-focus" aria-label={r.value}>
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

          {/* Catalog presets: one-tap apply */}
          {availablePresets.length > 0 && (
            <>
              <div className="px-1 pb-1 pt-3 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{t('v6AvailablePresets' as never)}</div>
              {availablePresets.map((p) => (
                <div key={p.id} className="flex items-center gap-2.5 border-b border-white/[0.07] px-1 py-2.5">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13.5px] font-medium text-white" title={p.title}>{p.title}</span>
                    <span className="line-clamp-1 text-[11px] text-white/40" title={p.description}>{p.description}</span>
                  </span>
                  <button
                    type="button"
                    onClick={() => applyPreset(p.id)}
                    className="shrink-0 rounded-[11px] px-3 py-1.5 text-[12px] font-semibold text-white transition-opacity hover:opacity-90 v6-focus"
                    style={{ background: 'linear-gradient(140deg, #FF8A4C, #FF5A1F)' }}
                  >
                    {t('v6Apply' as never)}
                  </button>
                </div>
              ))}
            </>
          )}

          {myRules.length === 0 && appliedPresets.length === 0 && availablePresets.length === 0 && (
            <div className="py-8 text-center text-[12px] text-white/40">{t('v6NoResults' as never)}</div>
          )}
        </div>
      </div>
    </div>
  );
}
