import { Power, CheckCircle2, Loader2 } from 'lucide-react';
import type { ConnectionStatus } from '../../stores/app-store';
import { formatDuration } from '../../lib/utils';

interface Props {
  status: ConnectionStatus;
  canConnect: boolean;
  connectTime: number;
  onConnect: () => void;
  t: (key: any) => string;
}

export default function ConnectionControls({ status, canConnect, connectTime, onConnect, t }: Props) {
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';

  return (
    <>
      {/* ── POWER BUTTON ── */}
      <div className="flex flex-col items-center mt-2 relative z-10 shrink-0">
        <button id="connect-button" onClick={onConnect}
          disabled={isConnecting || !canConnect}
          className="group relative w-40 h-40 flex items-center justify-center transition-all duration-150 cursor-pointer border-0 bg-transparent disabled:cursor-not-allowed">

          {isConnecting && (
            <svg className="absolute inset-[-12px] w-[calc(100%+24px)] h-[calc(100%+24px)] animate-spin-slow" viewBox="0 0 100 100">
              <circle cx="50" cy="50" r="46" fill="none" stroke="#000" strokeWidth="4" strokeDasharray="40 240" strokeLinecap="round" />
            </svg>
          )}

          <div className={`relative w-40 h-40 rounded-full flex items-center justify-center transition-all duration-150 border-[5px] border-black
            ${isConnected
              ? 'bg-black text-white shadow-[0_0_0_#000] scale-95'
              : isConnecting
                ? 'bg-white text-black shadow-[4px_4px_0_#000]'
                : canConnect
                  ? 'bg-white text-black shadow-[8px_8px_0_#000] hover:shadow-[10px_10px_0_#000] hover:-translate-y-1 hover:-translate-x-1 active:shadow-[2px_2px_0_#000] active:translate-y-[6px] active:translate-x-[6px]'
                  : 'bg-white/50 text-black/30 shadow-[4px_4px_0_rgba(0,0,0,0.3)]'
            }`}>
            <Power className={`w-16 h-16 transition-all duration-300 stroke-[3px] ${isConnecting ? 'animate-pulse' : ''}`} />
          </div>
        </button>

        {/* Status Label */}
        {isConnected ? (
          <div className="mt-4 px-6 py-3 bg-black rounded-2xl border-[3px] border-black shadow-[4px_4px_0_rgba(0,0,0,0.3)] hover:-translate-y-0.5 hover:shadow-[6px_6px_0_rgba(0,0,0,0.4)] transition-all">
            <p className="text-[13px] font-black tracking-widest uppercase text-emerald-400 text-center flex items-center justify-center gap-2">
              {t('protectedWorking')} <CheckCircle2 className="w-5 h-5 inline stroke-[3px]" />
            </p>
            <p className="text-[10px] text-emerald-400/50 text-center mt-1 uppercase tracking-widest font-bold">
              {t('timeValid')}: {formatDuration(connectTime)}
            </p>
          </div>
        ) : (
          <div className={`mt-4 px-6 py-3 rounded-2xl border-[3px] border-transparent transition-all duration-300 ${isConnecting ? 'bg-amber-100/50 border-amber-400/30' : ''}`}>
            <p className={`text-[13px] font-black tracking-widest uppercase flex items-center justify-center gap-2
              ${isConnecting ? 'text-amber-600 animate-pulse' : 'text-black/40'}`}>
              {isConnecting ? <><Loader2 className="w-4 h-4 inline animate-spin stroke-[3px]" /> {t('connecting')}</> : `${t('notConnected')} ❌`}
            </p>
          </div>
        )}
      </div>
    </>
  );
}
