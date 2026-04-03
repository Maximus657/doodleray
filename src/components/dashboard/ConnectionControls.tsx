import { useRef, useCallback } from 'react';
import { Power, Globe, Network, CheckCircle2, Loader2 } from 'lucide-react';
import type { ConnectionStatus, ProxyMode } from '../../stores/app-store';
import { formatDuration } from '../../lib/utils';

interface Props {
  status: ConnectionStatus;
  proxyMode: ProxyMode;
  canConnect: boolean;
  connectTime: number;
  onConnect: () => void;
  onModeSwitch: (mode: ProxyMode) => void;
  t: (key: any) => string;
}

export default function ConnectionControls({ status, proxyMode, canConnect, connectTime, onConnect, onModeSwitch, t }: Props) {
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';

  // Double-click protection: ignore rapid clicks within 1s
  const lastClickRef = useRef(0);
  const handleConnect = useCallback(() => {
    const now = Date.now();
    if (now - lastClickRef.current < 1000) return;
    lastClickRef.current = now;
    onConnect();
  }, [onConnect]);

  return (
    <>
      {/* ── PROXY MODE TOGGLE ── */}
      <div className="relative flex bg-black rounded-2xl p-1.5 shadow-inner w-full max-w-sm border-[3px] border-black shrink-0 mt-2 z-10">
        <div
          className={`absolute top-1.5 bottom-1.5 w-[calc(50%-6px)] bg-bg-primary rounded-xl transition-all duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] shadow-[2px_2px_0_rgba(0,0,0,0.4)] border-[2px] border-black ${
            proxyMode === 'tun' ? 'left-1/2' : 'left-1.5'
          }`}
        />
        <button onClick={() => onModeSwitch('system-proxy')}
          className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2 text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
            ${proxyMode === 'system-proxy' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
          <Globe className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'system-proxy' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('systemProxy')}</span>
        </button>
        <button onClick={() => onModeSwitch('tun')}
          className={`relative z-10 flex flex-1 items-center justify-center gap-2 py-2 text-[11px] font-black uppercase tracking-widest cursor-pointer transition-colors duration-300 select-none
            ${proxyMode === 'tun' ? 'text-black' : 'text-white/40 hover:text-white/80'}`}>
            <Network className={`w-4 h-4 transition-transform duration-300 ${proxyMode === 'tun' ? 'scale-110' : 'scale-100'}`} /> <span className="truncate">{t('tunMode')}</span>
        </button>
      </div>

      {/* ── POWER BUTTON ── */}
      <div className="flex flex-col items-center mt-2 relative z-10 shrink-0">
        <button id="connect-button" onClick={handleConnect}
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
