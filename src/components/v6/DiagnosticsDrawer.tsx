import { useEffect, useRef, useState } from 'react';
import { Activity, ChevronUp, Trash2, LifeBuoy, AlertCircle, Info, CheckCircle2, AlertTriangle } from 'lucide-react';
import type { LogEntry } from '../../stores/app-store';

type T = (key: never) => string;

interface Props {
  logs: LogEntry[];
  onClear: () => void;
  onExportSupportBundle: () => void;
  t: T;
}

const LEVEL_META: Record<LogEntry['level'], { color: string; icon: typeof Info }> = {
  info: { color: '#9aa3b4', icon: Info },
  success: { color: '#34d399', icon: CheckCircle2 },
  warning: { color: '#fbbf24', icon: AlertTriangle },
  error: { color: '#f87171', icon: AlertCircle },
};

/**
 * Bottom status rail + expandable diagnostics drawer. Surfaces the honest
 * issue count, recent events, and the redacted support-bundle export so health
 * never gets hidden behind a fake-green UI.
 */
export default function DiagnosticsDrawer({ logs, onClear, onExportSupportBundle, t }: Props) {
  const [open, setOpen] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  const issues = logs.filter((l) => l.level === 'error' || l.level === 'warning').length;
  const latest = logs[logs.length - 1];

  useEffect(() => {
    if (open) endRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs.length, open]);

  return (
    <div className="shrink-0 px-3 pb-2">
      <div className="v6-glass overflow-hidden rounded-xl">
        <div className="flex items-center gap-2 px-3 py-2">
          <button
            type="button"
            onClick={() => setOpen((o) => !o)}
            aria-expanded={open}
            className="flex min-w-0 flex-1 items-center gap-2 text-left v6-focus"
          >
            <Activity className="h-4 w-4 shrink-0 text-v6-muted" strokeWidth={2.2} />
            <span className="text-[11px] font-semibold uppercase tracking-wider text-v6-muted">
              {t('events' as never)}
            </span>
            {issues > 0 && (
              <span className="rounded-full bg-[#f8717120] px-1.5 py-0.5 text-[9px] font-semibold text-[#fca5a5]">
                {issues} {t('issues' as never)}
              </span>
            )}
            {latest && !open && (
              <span className="min-w-0 flex-1 truncate text-[11px] text-v6-muted" style={{ color: LEVEL_META[latest.level].color }}>
                {latest.message}
              </span>
            )}
            <ChevronUp className={`ml-auto h-4 w-4 shrink-0 text-v6-muted transition-transform ${open ? '' : 'rotate-180'}`} strokeWidth={2.2} />
          </button>

          <button
            type="button"
            onClick={onExportSupportBundle}
            title={t('supportBundle' as never)}
            className="flex items-center gap-1.5 rounded-lg bg-white/[0.06] px-2.5 py-1.5 text-[10px] font-medium text-v6-text hover:bg-white/10 v6-focus"
          >
            <LifeBuoy className="h-3.5 w-3.5" strokeWidth={2.2} />
            <span className="hidden sm:inline">{t('supportBundle' as never)}</span>
          </button>
        </div>

        <div className="drawer-collapse" data-open={open}>
          <div className="drawer-collapse-inner border-t border-v6-line">
            <div className="flex items-center justify-between px-3 py-1.5">
              <span className="text-[10px] uppercase tracking-wider text-v6-muted">{logs.length}</span>
              <button
                type="button"
                onClick={onClear}
                className="flex items-center gap-1 text-[10px] text-v6-muted hover:text-v6-text v6-focus"
              >
                <Trash2 className="h-3 w-3" strokeWidth={2.2} /> {t('clear' as never)}
              </button>
            </div>
            <div className="max-h-64 space-y-0.5 overflow-y-auto px-3 pb-3 font-mono">
              {logs.length === 0 ? (
                <div className="py-6 text-center text-[11px] text-v6-muted">{t('v6NoEvents' as never)}</div>
              ) : (
                logs.map((log) => {
                  const meta = LEVEL_META[log.level];
                  const Icon = meta.icon;
                  return (
                    <div key={log.id} className="flex items-start gap-2 rounded-md px-1.5 py-1 text-[10.5px] leading-snug hover:bg-white/[0.04]">
                      <Icon className="mt-0.5 h-3 w-3 shrink-0" style={{ color: meta.color }} strokeWidth={2.2} />
                      <span className="shrink-0 tabular-nums text-v6-muted/60">{log.time}</span>
                      <span className="min-w-0 flex-1 break-words text-v6-text/85">{log.message}</span>
                    </div>
                  );
                })
              )}
              <div ref={endRef} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
