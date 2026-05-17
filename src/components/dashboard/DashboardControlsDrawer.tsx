import { useState } from 'react';
import {
  Activity,
  ChevronDown,
  CheckCircle2,
  CircleHelp,
  Globe,
  Network,
  Rss,
  SlidersHorizontal,
} from 'lucide-react';
import type { ConnectionStatus, ProxyMode, SpeedPoint, Subscription, SystemProxyMode } from '../../stores/app-store';
import { formatBytes } from '../../lib/utils';
import StatsPanel from './StatsPanel';

interface Props {
  status: ConnectionStatus;
  proxyMode: ProxyMode;
  systemProxyMode: SystemProxyMode;
  connectTime: number;
  currentDownload: number;
  currentUpload: number;
  totalDown: number;
  totalUp: number;
  speedHistory: SpeedPoint[];
  showStats: boolean;
  activeSubscription: Subscription | null;
  activeSubscriptionServerCount: number;
  onModeSwitch: (mode: ProxyMode) => void;
  onSystemProxyModeChange: (mode: SystemProxyMode) => void;
  t: (key: any) => string;
}

function formatTrafficQuotaBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024 * 1024) return formatBytes(bytes);

  const gb = bytes / (1024 * 1024 * 1024);
  const isWholeGb = Math.abs(gb - Math.round(gb)) < 0.001;
  return `${isWholeGb ? Math.round(gb).toString() : gb.toFixed(2)} GB`;
}

export default function DashboardControlsDrawer({
  status,
  proxyMode,
  systemProxyMode,
  connectTime,
  currentDownload,
  currentUpload,
  totalDown,
  totalUp,
  speedHistory,
  showStats,
  activeSubscription,
  activeSubscriptionServerCount,
  onModeSwitch,
  onSystemProxyModeChange,
  t,
}: Props) {
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';
  const [modeHelp, setModeHelp] = useState<ProxyMode | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);

  const activeModeLabel = proxyMode === 'tun' ? t('fullDeviceMode') : t('systemProxy');
  const subscriptionTraffic = activeSubscription?.traffic;
  const subscriptionUsed = subscriptionTraffic
    ? subscriptionTraffic.upload + subscriptionTraffic.download
    : 0;
  const subscriptionTotal = subscriptionTraffic?.total;
  const hasReliableTraffic = !!subscriptionTraffic && !!subscriptionTotal && subscriptionTotal > 0;
  const subscriptionPercent = hasReliableTraffic
    ? Math.min(100, (subscriptionUsed / subscriptionTotal) * 100)
    : 0;
  const subscriptionExpire = subscriptionTraffic?.expire
    ? new Date(subscriptionTraffic.expire * 1000).toLocaleDateString('ru-RU')
    : null;
  const subscriptionUpdated = activeSubscription?.updatedAt
    ? new Date(activeSubscription.updatedAt).toLocaleDateString('ru-RU')
    : null;

  const switchMode = (mode: ProxyMode) => {
    setModeHelp(null);
    onModeSwitch(mode);
  };

  const toggleModeHelp = (mode: ProxyMode) => {
    setModeHelp((current) => current === mode ? null : mode);
  };

  return (
    <div className="relative z-10 w-full max-w-sm">
      <div
        data-open={detailsOpen ? 'true' : 'false'}
        aria-hidden={!detailsOpen}
        inert={!detailsOpen ? true : undefined}
        className="drawer-collapse"
      >
        <div className="drawer-collapse-inner">
          <div className="flex w-full flex-col gap-3 pb-3">
            <div className="grid w-full gap-2">
              {([
                {
                  mode: 'system-proxy' as const,
                  icon: Globe,
                  title: t('proxyRecommendedTitle'),
                  badge: t('recommended'),
                  body: t('modeHelpProxyBody'),
                  help: t('modeHelpProxyTitle'),
                },
                {
                  mode: 'tun' as const,
                  icon: Network,
                  title: t('tunAdvancedTitle'),
                  badge: t('fullDeviceMode'),
                  body: t('modeHelpTunBody'),
                  help: t('modeHelpTunTitle'),
                },
              ]).map((item) => {
                const Icon = item.icon;
                const selected = proxyMode === item.mode;
                return (
                  <div
                    key={item.mode}
                    className={`rounded-xl border-[3px] p-2.5 transition-all duration-300 ${
                      selected
                        ? 'border-black bg-white shadow-[4px_4px_0_#000]'
                        : 'border-black/45 bg-white/60 hover:border-black hover:bg-white'
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <button
                        type="button"
                        onClick={() => switchMode(item.mode)}
                        className="flex min-w-0 flex-1 cursor-pointer items-start gap-2 text-left"
                      >
                        <span className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border-[2px] border-black ${
                          selected ? 'bg-bg-primary text-black' : 'bg-black/5 text-black/50'
                        }`}>
                          <Icon className="h-4 w-4 stroke-[3px]" />
                        </span>
                        <span className="min-w-0">
                          <span className="flex items-center gap-1.5">
                            <span className="truncate text-[11px] font-black uppercase tracking-widest text-black">{item.title}</span>
                            {selected && <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-500 stroke-[3px]" />}
                          </span>
                          <span className={`mt-1 inline-flex rounded-md border-[2px] border-black px-1.5 py-0.5 text-[8px] font-black uppercase tracking-widest ${
                            selected ? 'bg-emerald-300 text-black' : 'bg-white text-black/50'
                          }`}>
                            {item.badge}
                          </span>
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => toggleModeHelp(item.mode)}
                        aria-label={item.help}
                        aria-expanded={modeHelp === item.mode}
                        className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border-[2px] border-black bg-white text-black transition-all hover:bg-black hover:text-white"
                      >
                        <CircleHelp className="h-3.5 w-3.5 stroke-[3px]" />
                      </button>
                    </div>
                    {item.mode === 'tun' && selected && (
                      <p className="mt-2 rounded-lg border-[2px] border-amber-500 bg-amber-200 px-2 py-1 text-[9px] font-black uppercase tracking-widest text-black">
                        {t('adminPermissionHint')}
                      </p>
                    )}
                  </div>
                );
              })}
            </div>

          {modeHelp && (
            <div className="relative z-30 w-full rounded-xl border-[3px] border-black bg-white p-3 text-left shadow-[4px_4px_0_#000] animate-slide-up">
              <p className="text-[11px] font-black uppercase tracking-widest text-black">
                {modeHelp === 'tun' ? t('modeHelpTunTitle') : t('modeHelpProxyTitle')}
              </p>
              <p className="mt-1 text-[10px] font-bold leading-relaxed text-black/65">
                {modeHelp === 'tun' ? t('modeHelpTunBody') : t('modeHelpProxyBody')}
              </p>
            </div>
          )}

          {proxyMode === 'system-proxy' && (
            <div className="relative z-10 flex w-full items-center gap-1.5 rounded-xl border-[3px] border-black bg-white p-1.5 shadow-[4px_4px_0_#000]">
              {([
                ['set', 'systemProxyShortSet'],
                ['unchanged', 'systemProxyShortKeep'],
                ['clear', 'systemProxyShortClear'],
              ] as const).map(([mode, labelKey]) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => onSystemProxyModeChange(mode)}
                  disabled={isConnecting || isConnected}
                  title={
                    mode === 'set'
                      ? t('systemProxySet')
                      : mode === 'unchanged'
                        ? t('systemProxyUnchanged')
                        : t('systemProxyClear')
                  }
                  className={`flex-1 rounded-lg border-[2px] border-black px-2 py-1.5 text-[9px] font-black uppercase tracking-widest transition-all disabled:cursor-not-allowed disabled:opacity-50 ${
                    systemProxyMode === mode
                      ? 'bg-black text-white shadow-[2px_2px_0_rgba(0,0,0,0.3)]'
                      : 'bg-bg-primary text-black hover:-translate-y-0.5 hover:shadow-[2px_2px_0_#000]'
                  }`}
                >
                  {t(labelKey)}
                </button>
              ))}
            </div>
          )}

          {activeSubscription && (
            <div className="w-full rounded-xl border-[3px] border-black bg-white p-3 shadow-[4px_4px_0_#000]">
              <div className="flex items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2">
                  <Rss className="h-4 w-4 shrink-0 stroke-[3px]" />
                  <span className="truncate text-[11px] font-black uppercase tracking-widest text-black">
                    {activeSubscription.name}
                  </span>
                </div>
                <span className="shrink-0 rounded-lg bg-black px-2 py-1 text-[9px] font-black uppercase tracking-widest text-white">
                  {activeSubscriptionServerCount}
                </span>
              </div>
              {hasReliableTraffic ? (
                <div className="mt-3">
                  <div className="h-2.5 overflow-hidden rounded-full border-[2px] border-black bg-black/10">
                    <div className="h-full bg-emerald-400" style={{ width: `${subscriptionPercent}%` }} />
                  </div>
                  <div className="mt-1.5 flex items-center justify-between gap-2 text-[9px] font-black uppercase tracking-widest text-black/55">
                    <span className="truncate">
                      {`${formatTrafficQuotaBytes(subscriptionUsed)} / ${formatTrafficQuotaBytes(subscriptionTotal!)}`}
                    </span>
                    {subscriptionExpire && <span className="shrink-0">{t('validUntil')} {subscriptionExpire}</span>}
                  </div>
                  {subscriptionUpdated && (
                    <p className="mt-1 text-[8px] font-black uppercase tracking-widest text-black/35">
                      {t('lastUpdated')} {subscriptionUpdated}
                    </p>
                  )}
                </div>
              ) : (
                <p className="mt-2 text-[10px] font-black uppercase tracking-widest text-black/40">
                  {t('trafficUnavailable')}
                </p>
              )}
            </div>
          )}

            {showStats && (isConnected || speedHistory.length > 0) && (
              <div className="w-full">
                <div className="mb-2 flex items-center gap-2 px-1 text-[10px] font-black uppercase tracking-widest text-black/50">
                  <Activity className="h-3.5 w-3.5 stroke-[3px]" />
                  {t('liveThroughput')}
                </div>
                <StatsPanel
                  currentDownload={currentDownload}
                  currentUpload={currentUpload}
                  totalDown={totalDown}
                  totalUp={totalUp}
                  connectTime={connectTime}
                  proxyMode={proxyMode}
                  speedHistory={speedHistory}
                  compact
                  t={t}
                />
              </div>
            )}
          </div>
        </div>
      </div>

      <button
        type="button"
        onClick={() => setDetailsOpen((open) => {
          if (open) setModeHelp(null);
          return !open;
        })}
        aria-expanded={detailsOpen}
        className="flex w-full items-center justify-between gap-3 rounded-xl border-[2px] border-black/35 bg-white/72 px-3.5 py-2.5 text-black shadow-[0_2px_0_rgba(0,0,0,0.18)] backdrop-blur transition-all hover:border-black/65 hover:bg-white active:translate-y-0.5 active:shadow-none"
      >
        <span className="flex min-w-0 items-center gap-2 text-[10px] font-black uppercase tracking-widest">
          <SlidersHorizontal className="h-4 w-4 shrink-0 stroke-[3px]" />
          <span className="truncate">{t('connectionControls')}</span>
        </span>
        <span className="flex shrink-0 items-center gap-2 text-[9px] font-black uppercase tracking-widest text-black/55">
          {activeModeLabel}
          <ChevronDown className={`h-4 w-4 stroke-[3px] transition-transform ${detailsOpen ? 'rotate-180' : ''}`} />
        </span>
      </button>
    </div>
  );
}
