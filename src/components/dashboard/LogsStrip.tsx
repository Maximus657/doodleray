import { ScrollText, ChevronDown, ChevronUp } from 'lucide-react';
import type { LogEntry } from '../../stores/app-store';
import type { RefObject } from 'react';

interface Props {
  logs: LogEntry[];
  showLogs: boolean;
  onToggleLogs: () => void;
  onClearLogs: () => void;
  logsEndRef: RefObject<HTMLDivElement | null>;
  t: (key: any) => string;
}

export default function LogsStrip({ logs, showLogs, onToggleLogs, onClearLogs, logsEndRef, t }: Props) {
  const issueCount = logs.filter((log) => log.level === 'error' || log.level === 'warning').length;
  const hasIssues = issueCount > 0;

  return (
    <div className={`relative z-30 overflow-hidden flex flex-col transition-all duration-300 shrink-0 border-t-[3px] border-black
      ${hasIssues ? 'bg-red-50' : 'bg-white'}
      ${showLogs ? 'h-40' : 'h-8'}`}>
      <div
        role="button"
        tabIndex={0}
        onClick={onToggleLogs}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') onToggleLogs();
        }}
        className="flex h-8 items-center justify-between px-4 shrink-0 cursor-pointer hover:bg-black/5 transition-all"
      >
        <div className="flex items-center gap-2 text-black font-black uppercase tracking-widest text-[10px]">
          <ScrollText className={`w-3.5 h-3.5 ${hasIssues ? 'text-red-600' : ''}`} />
          <span>{hasIssues ? t('issues') : t('events')}</span>
          {logs.length > 0 && (
            <span className={`${hasIssues ? 'text-red-600' : 'text-black/45'}`}>
              ({hasIssues ? issueCount : logs.length})
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
          {showLogs && logs.length > 0 && (
            <button onClick={(e) => { e.stopPropagation(); onClearLogs(); }} className="text-[10px] uppercase font-black text-black/50 hover:text-black cursor-pointer">{t('clear')}</button>
          )}
          {showLogs ? <ChevronDown className="w-4 h-4 text-black stroke-[3px]" /> : <ChevronUp className="w-4 h-4 text-black stroke-[3px]" />}
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-4 pb-2 font-mono text-[11px] font-black uppercase space-y-1">
        {logs.length === 0 ? (
          <p className="text-black/40 py-2 text-center text-[10px]">{t('noLogsYet')}</p>
        ) : (
          logs.map((log) => (
            <div key={log.id} className="flex gap-3 border-b border-black/5 pb-1">
              <span className="text-black/40 shrink-0 whitespace-nowrap">{log.time}</span>
              <span className={`break-words ${
                log.level === 'error' ? 'text-red-600' :
                log.level === 'warning' ? 'text-orange-600' :
                log.level === 'success' ? 'text-emerald-700' :
                'text-black'
              }`}>{log.message}</span>
            </div>
          ))
        )}
        <div ref={logsEndRef} />
      </div>
    </div>
  );
}
