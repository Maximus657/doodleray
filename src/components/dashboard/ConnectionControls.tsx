import { CheckCircle2, Loader2, Power, Unplug } from 'lucide-react';
import type { ConnectionStatus } from '../../stores/app-store';

interface Props {
  status: ConnectionStatus;
  canConnect: boolean;
  connectionStepLabel?: string | null;
  onConnect: () => void;
  t: (key: any) => string;
}

export default function ConnectionControls({
  status,
  canConnect,
  connectionStepLabel,
  onConnect,
  t,
}: Props) {
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';
  const isDisconnecting = status === 'disconnecting';
  const primaryLabel = isConnected
    ? t('disconnect')
    : isDisconnecting
      ? t('connectionDisconnecting')
      : isConnecting
        ? t('cancel')
        : t('connect');
  const secondaryLabel = isConnected
    ? t('connected')
    : isConnecting
      ? connectionStepLabel || t('connectionStarting')
      : isDisconnecting
        ? null
        : null;
  const buttonTitle = isConnected
    ? t('disconnect')
    : isConnecting
      ? t('cancel')
      : isDisconnecting
        ? t('disconnecting')
        : t('connect');

  return (
    <div className="relative z-10 mt-5 mb-1 flex w-full max-w-sm shrink-0 flex-col items-center">
      <button
        type="button"
        id="connect-button"
        title={buttonTitle}
        aria-label={buttonTitle}
        onClick={onConnect}
        disabled={status === 'disconnected' && !canConnect}
        className={`group relative flex h-44 w-44 flex-col items-center justify-center gap-2 overflow-hidden rounded-full border-[5px] text-center transition-all duration-200 will-change-transform disabled:cursor-not-allowed ${
          isConnected
            ? 'animate-vpn-connected cursor-pointer border-black bg-black text-white shadow-[0_0_0_6px_rgba(16,185,129,0.18),6px_6px_0_rgba(0,0,0,0.35)] hover:-translate-x-1 hover:-translate-y-1 hover:bg-danger hover:text-black hover:shadow-[0_0_0_7px_rgba(248,113,113,0.22),9px_9px_0_rgba(0,0,0,0.45)] active:translate-x-[4px] active:translate-y-[4px] active:shadow-[3px_3px_0_rgba(0,0,0,0.5)]'
            : isConnecting || isDisconnecting
              ? 'cursor-pointer border-black bg-white text-black shadow-[0_0_0_6px_rgba(251,191,36,0.24),6px_6px_0_#000] hover:-translate-y-1 hover:shadow-[0_0_0_7px_rgba(251,191,36,0.28),8px_8px_0_#000] active:translate-y-[4px] active:shadow-[3px_3px_0_#000]'
              : canConnect
                ? 'border-black bg-white text-black shadow-[8px_8px_0_#000] hover:-translate-x-1 hover:-translate-y-1 hover:shadow-[10px_10px_0_#000] active:translate-x-[5px] active:translate-y-[5px] active:shadow-[3px_3px_0_#000]'
                : 'border-black/35 bg-white/60 text-black/35 shadow-[4px_4px_0_rgba(0,0,0,0.25)]'
        }`}
      >
        {(isConnecting || isDisconnecting) && (
          <svg className="pointer-events-none absolute inset-[-13px] h-[calc(100%+26px)] w-[calc(100%+26px)] animate-spin-slow" viewBox="0 0 100 100">
            <circle cx="50" cy="50" r="46" fill="none" stroke="#000" strokeWidth="4" strokeDasharray="42 245" strokeLinecap="round" />
          </svg>
        )}
        {isConnected && (
          <span className="pointer-events-none absolute inset-[-8px] rounded-full border-[4px] border-emerald-400/35" />
        )}
        <span className={`flex h-16 w-16 shrink-0 items-center justify-center rounded-full border-[3px] transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${
          isConnected
            ? 'animate-vpn-pop border-emerald-300 bg-emerald-400 text-black shadow-[0_0_0_5px_rgba(52,211,153,0.22)] group-hover:border-black group-hover:bg-black group-hover:text-white group-hover:shadow-[0_0_0_5px_rgba(0,0,0,0.12)]'
            : canConnect
              ? 'border-black bg-black text-white shadow-[0_0_0_5px_rgba(0,0,0,0.08)] group-hover:scale-105'
              : 'border-black/30 bg-black/15 text-black/35'
        }`}>
          {isConnecting ? (
            <Loader2 className="h-9 w-9 animate-spin stroke-[3px]" />
          ) : isDisconnecting ? (
            <Unplug className="h-9 w-9 animate-pulse stroke-[3px]" />
          ) : isConnected ? (
            <span className="relative h-10 w-10">
              <CheckCircle2 className="absolute inset-0 h-10 w-10 stroke-[3px] transition-all duration-200 group-hover:scale-75 group-hover:opacity-0" />
              <Unplug className="absolute inset-0 h-10 w-10 scale-75 opacity-0 stroke-[3px] transition-all duration-200 group-hover:scale-100 group-hover:opacity-100" />
            </span>
          ) : (
            <Power className="h-9 w-9 stroke-[3px]" />
          )}
        </span>

        <span className="flex w-full min-w-0 flex-col items-center justify-center px-5">
          <span className="max-w-[138px] text-center text-[17px] font-black uppercase leading-[0.95] tracking-tight [overflow-wrap:anywhere]">
            {primaryLabel}
          </span>
          {secondaryLabel && (
            <span className={`mt-1 max-w-[132px] rounded-full border-[2px] px-2 py-0.5 text-center text-[8px] font-black uppercase leading-tight tracking-widest transition-colors ${
              isConnected
                ? 'border-white/30 bg-white/10 text-white/70 group-hover:border-black group-hover:bg-white group-hover:text-black'
                : isConnecting
                  ? 'border-black bg-amber-300 text-black'
                  : 'border-black/20 bg-white/75 text-black/45'
            }`}>
              {secondaryLabel}
            </span>
          )}
        </span>
      </button>
    </div>
  );
}
