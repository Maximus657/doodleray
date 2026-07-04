import { useEffect, useRef, useState } from 'react';
import { Activity, ChevronUp, Trash2, Stethoscope, AlertCircle, Info, CheckCircle2, AlertTriangle } from 'lucide-react';
import type { LogEntry } from '../../stores/app-store';

type T = (key: never) => string;

interface Props {
  logs: LogEntry[];
  onClear: () => void;
  onOpenDiagnostics: () => void;
  t: T;
}

const LEVEL_META: Record<LogEntry['level'], { color: string; icon: typeof Info }> = {
  info: { color: 'rgba(255,255,255,0.55)', icon: Info },
  success: { color: '#3ddc84', icon: CheckCircle2 },
  warning: { color: '#ffb02e', icon: AlertTriangle },
  error: { color: '#ff6b5a', icon: AlertCircle },
  debug: { color: 'rgba(255,255,255,0.35)', icon: Info },
};

/**
 * Bottom status rail with the honest issue count and support-bundle export.
 * The expanded log view opens as an overlay popover ABOVE the bar (absolute,
 * out of flow) so expanding never shifts or breaks the dashboard layout.
 */
export default function DiagnosticsDrawer({ logs, onClear, onOpenDiagnostics, t }: Props) {
  const [open, setOpen] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  // Service/diagnostic chatter stays out of the user-facing list; it is still
  // kept in the store for QA snapshots and support bundles.
  const visibleLogs = logs.filter((l) => l.level !== 'debug');
  const issues = visibleLogs.filter((l) => l.level === 'error' || l.level === 'warning').length;
  const latest = visibleLogs[visibleLogs.length - 1];

  // Scroll only the log list itself — scrollIntoView would also scroll every
  // scrollable ancestor and visually shift the whole dashboard.
  useEffect(() => {
    if (open && listRef.current) listRef.current.scrollTop = listRef.current.scrollHeight;
  }, [logs.length, open]);

  return (
    <div className="relative shrink-0">
      {/* Expanded log popover (overlay, no layout shift) */}
      {open && (
        <>
          <div className="fixed inset-0 z-20" onClick={() => setOpen(false)} />
          <div className="v6-modal v6-fadein absolute inset-x-0 bottom-[calc(100%+10px)] z-30 overflow-hidden rounded-[20px]">
            <div className="flex items-center justify-between px-4 py-2.5">
              <span className="text-[11px] font-semibold uppercase tracking-wider text-white/50">
                {t('events' as never)} <span className="ml-1 tabular-nums text-white/35">{visibleLogs.length}</span>
              </span>
              <button
                type="button"
                onClick={onClear}
                className="flex items-center gap-1 rounded-lg px-2 py-1 text-[11px] text-white/50 hover:bg-white/[0.08] hover:text-white v6-focus"
              >
                <Trash2 className="h-3 w-3" strokeWidth={2.2} /> {t('clear' as never)}
              </button>
            </div>
            <div ref={listRef} className="max-h-[300px] space-y-0.5 overflow-y-auto border-t border-white/[0.07] px-3 py-2 font-mono">
              {visibleLogs.length === 0 ? (
                <div className="py-6 text-center text-[11px] text-white/40">{t('v6NoEvents' as never)}</div>
              ) : (
                visibleLogs.map((log) => {
                  const meta = LEVEL_META[log.level];
                  const Icon = meta.icon;
                  return (
                    <div key={log.id} className="flex items-start gap-2 rounded-md px-1.5 py-1 text-[10.5px] leading-snug hover:bg-white/[0.04]">
                      <Icon className="mt-0.5 h-3 w-3 shrink-0" style={{ color: meta.color }} strokeWidth={2.2} />
                      <span className="shrink-0 tabular-nums text-white/35">{log.time}</span>
                      <span className="min-w-0 flex-1 break-words text-white/85">{log.message}</span>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}

      {/* Status bar (hard-fixed height so expanding can never shift layout) */}
      <div className="v6-glass flex h-[52px] items-center gap-2 rounded-[20px] px-3">
        <button
          type="button"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-xl px-1 text-left hover:bg-white/[0.04] v6-focus"
        >
          <Activity className="h-4 w-4 shrink-0 text-white/50" strokeWidth={2.2} />
          <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wider text-white/50">
            {t('events' as never)}
          </span>
          {issues > 0 && (
            <span className="shrink-0 rounded-full px-1.5 py-0.5 text-[9px] font-semibold" style={{ background: 'rgba(255,107,90,0.16)', color: '#ffb3a8' }}>
              {issues} {t('issues' as never)}
            </span>
          )}
          {latest && !open && (
            <span className="min-w-0 flex-1 truncate text-[11px]" style={{ color: LEVEL_META[latest.level].color }}>
              {latest.message}
            </span>
          )}
          <ChevronUp className={`ml-auto h-4 w-4 shrink-0 text-white/50 transition-transform ${open ? 'rotate-180' : ''}`} strokeWidth={2.2} />
        </button>

        <button
          type="button"
          onClick={onOpenDiagnostics}
          title={t('v6Diag' as never)}
          className="v6-hover-bright flex shrink-0 items-center gap-1.5 rounded-xl border border-white/[0.1] bg-white/[0.06] px-2.5 py-1.5 text-[11px] font-medium text-white v6-focus"
        >
          <Stethoscope className="h-3.5 w-3.5" strokeWidth={2.2} />
          <span className="hidden lg:inline">{t('v6Diag' as never)}</span>
        </button>
      </div>
    </div>
  );
}
