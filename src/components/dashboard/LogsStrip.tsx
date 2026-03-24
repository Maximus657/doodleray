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
  return (
    <div className={`bg-white border-t-[4px] border-black overflow-hidden flex flex-col transition-all duration-300 shrink-0
      ${showLogs ? 'h-40' : 'h-10'}`}>
      <button onClick={onToggleLogs}
        className="flex items-center justify-between px-4 py-2 shrink-0 cursor-pointer hover:bg-black/5 transition-all">
        <div className="flex items-center gap-2 text-black font-black uppercase tracking-widest text-[11px]">
          <ScrollText className="w-4 h-4" />
          <span>{t('terminal')}</span>
          {logs.length > 0 && <span className="text-black/50">({logs.length})</span>}
        </div>
        <div className="flex items-center gap-3">
          {logs.length > 0 && (
            <button onClick={(e) => { e.stopPropagation(); onClearLogs(); }} className="text-[10px] uppercase font-black text-black/50 hover:text-black cursor-pointer">{t('clear')}</button>
          )}
          {showLogs ? <ChevronDown className="w-5 h-5 text-black stroke-[3px]" /> : <ChevronUp className="w-5 h-5 text-black stroke-[3px]" />}
        </div>
      </button>
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
