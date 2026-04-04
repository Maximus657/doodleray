import { ArrowDown, ArrowUp, Timer, Shield, Wifi } from 'lucide-react';
import {
  AreaChart,
  Area,
  ResponsiveContainer,
  CartesianGrid,
  YAxis,
} from 'recharts';
import type { ProxyMode, SpeedPoint } from '../../stores/app-store';
import { formatSpeed, formatDuration, formatBytes } from '../../lib/utils';

interface Props {
  currentDownload: number;
  currentUpload: number;
  totalDown: number;
  totalUp: number;
  connectTime: number;
  proxyMode: ProxyMode;
  speedHistory: SpeedPoint[];
  t: (key: any) => string;
}

export default function StatsPanel({ currentDownload, currentUpload, totalDown, totalUp, connectTime, proxyMode, speedHistory, t }: Props) {
  const displayData = speedHistory.slice(-30);

  return (
    <>
      {/* ── STATS CARDS ── */}
      <div className="grid grid-cols-3 gap-4 w-full max-w-md mt-4 shrink-0 animate-slide-up relative z-10">
        <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
          <ArrowDown className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
          <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatSpeed(currentDownload)}</p>
          <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">{t('download')}</p>
          <p className="text-[10px] font-black font-mono text-black/40 mt-1">{formatBytes(totalDown)}</p>
        </div>
        <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
          <ArrowUp className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
          <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatSpeed(currentUpload)}</p>
          <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">{t('upload')}</p>
          <p className="text-[10px] font-black font-mono text-black/40 mt-1">{formatBytes(totalUp)}</p>
        </div>
        <div className="bg-white rounded-2xl p-3 text-center border-[3px] border-black shadow-[4px_4px_0_#000]">
          <Timer className="w-5 h-5 mx-auto text-black mb-1 stroke-[3px]" />
          <p className="text-xl font-black text-black tabular-nums tracking-tighter">{formatDuration(connectTime)}</p>
          <p className="text-[10px] font-black text-black/60 uppercase tracking-widest mt-0.5">{t('time')}</p>
          <p className="text-[10px] font-black font-mono text-black/40 mt-1 flex items-center justify-center gap-1">
            <Shield className="w-3 h-3" /> {proxyMode === 'tun' ? 'TUN' : 'PROXY'}
          </p>
        </div>
      </div>

      {/* ── SPEED GRAPH ── */}
      {displayData.length > 2 && (
        <div className="w-full max-w-md card rounded-2xl p-3 shrink-0 animate-slide-up relative z-10 pointer-events-none">
          <div className="flex items-center justify-between mb-2 px-1">
            <span className="text-[10px] font-bold text-text-on-dark-muted uppercase tracking-widest flex items-center gap-1">
              <Wifi className="w-3 h-3" /> {t('liveThroughput')}
            </span>
            <div className="flex items-center gap-3">
              <span className="text-[9px] text-emerald-400 font-bold flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-emerald-400 inline-block" /> DL</span>
              <span className="text-[9px] text-blue-400 font-bold flex items-center gap-1"><span className="w-1.5 h-1.5 rounded-full bg-blue-400 inline-block" /> UL</span>
            </div>
          </div>
          <div className="h-24 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={displayData} margin={{ top: 2, right: 5, left: 0, bottom: 0 }}>
                <defs>
                  <linearGradient id="dlGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#34d399" stopOpacity={0.5} />
                    <stop offset="100%" stopColor="#34d399" stopOpacity={0.05} />
                  </linearGradient>
                  <linearGradient id="ulGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#60a5fa" stopOpacity={0.4} />
                    <stop offset="100%" stopColor="#60a5fa" stopOpacity={0.05} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
                <YAxis hide domain={[0, 'auto']} />
                <Area type="monotone" dataKey="download" stroke="#34d399" fill="url(#dlGrad)" strokeWidth={2} dot={false} isAnimationActive={true} animationDuration={900} animationEasing="ease-in-out" connectNulls />
                <Area type="monotone" dataKey="upload" stroke="#60a5fa" fill="url(#ulGrad)" strokeWidth={1.5} dot={false} isAnimationActive={true} animationDuration={900} animationEasing="ease-in-out" connectNulls />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </div>
      )}
    </>
  );
}
