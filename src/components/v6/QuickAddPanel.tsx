import { Plus, Loader2, ClipboardPaste, X } from 'lucide-react';

type T = (key: never) => string;

interface Props {
  value: string;
  onChange: (v: string) => void;
  onAdd: () => void;
  onPaste: () => void;
  onClose: () => void;
  importing: boolean;
  kind: 'subscription' | 'link' | 'unknown';
  hint: string;
  t: T;
}

/** Popover for pasting a subscription URL or a single proxy link. */
export default function QuickAddPanel({ value, onChange, onAdd, onPaste, onClose, importing, kind, hint, t }: Props) {
  const canAdd = !importing && !!value.trim() && kind !== 'unknown';
  const badge = kind === 'subscription' ? '#34d399' : kind === 'link' ? '#fbbf24' : '#6b7488';
  return (
    <>
      <div className="fixed inset-0 z-40 bg-black/40 backdrop-blur-sm" onClick={onClose} />
      <div className="v6-glass fixed left-1/2 top-16 z-50 w-[min(380px,calc(100vw-32px))] -translate-x-1/2 rounded-2xl p-4 shadow-2xl animate-slide-up">
        <div className="flex items-start justify-between">
          <div>
            <p className="text-[12.5px] font-semibold text-v6-text">{t('pasteToAddTitle' as never)}</p>
            <p className="mt-0.5 text-[11px] leading-snug text-v6-muted">{t('pasteToAddDesc' as never)}</p>
          </div>
          <button type="button" onClick={onClose} aria-label={t('cancel' as never)} className="grid h-6 w-6 place-items-center rounded-md text-v6-muted hover:bg-white/10 hover:text-v6-text v6-focus">
            <X className="h-4 w-4" strokeWidth={2.2} />
          </button>
        </div>

        <div className="mt-3 flex gap-2">
          <input
            type="text"
            value={value}
            autoFocus
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && canAdd) { onAdd(); } }}
            placeholder={t('pasteHint' as never)}
            className="v6-glass-inset min-w-0 flex-1 rounded-lg px-3 py-2.5 text-[12px] text-v6-text placeholder:text-v6-muted/70 v6-focus"
          />
          <button
            type="button"
            onClick={onPaste}
            aria-label="Paste"
            className="grid h-[42px] w-[42px] shrink-0 place-items-center rounded-lg bg-white/[0.06] text-v6-muted hover:bg-white/10 hover:text-v6-text v6-focus"
          >
            <ClipboardPaste className="h-4 w-4" strokeWidth={2.2} />
          </button>
        </div>

        <p className="mt-2 inline-flex rounded-md px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wider" style={{ color: badge, background: `${badge}18` }}>
          {hint}
        </p>

        <button
          type="button"
          onClick={onAdd}
          disabled={!canAdd}
          className="mt-3 flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-[#7c6cff] to-[#5b8cff] py-2.5 text-[12px] font-semibold text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40 v6-focus"
        >
          {importing ? <><Loader2 className="h-4 w-4 v6-orb-spin" /> {t('adding' as never)}</> : <><Plus className="h-4 w-4" strokeWidth={2.6} /> {t('add' as never)}</>}
        </button>
      </div>
    </>
  );
}
