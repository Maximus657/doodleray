import { useState } from 'react';
import {
  Activity,
  ChevronDown,
  CheckCircle2,
  CircleHelp,
  Globe,
  Network,
  SlidersHorizontal,
} from 'lucide-react';
import type { ConnectionStatus, ProxyMode, SpeedPoint, SystemProxyMode } from '../../stores/app-store';
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
  socksPort: number;
  httpPort: number;
  speedHistory: SpeedPoint[];
  showStats: boolean;
  onModeSwitch: (mode: ProxyMode, systemProxyMode?: SystemProxyMode) => void;
  t: (key: any) => string;
}

type ProductModeKey = 'browser-apps' | 'whole-computer' | 'manual-proxy';

export default function DashboardControlsDrawer({
  status,
  proxyMode,
  systemProxyMode,
  connectTime,
  currentDownload,
  currentUpload,
  totalDown,
  totalUp,
  socksPort,
  httpPort,
  speedHistory,
  showStats,
  onModeSwitch,
  t,
}: Props) {
  const isConnected = status === 'connected';
  const isConnecting = status === 'connecting';
  const isBusy = isConnecting || status === 'disconnecting';
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [openHelpMode, setOpenHelpMode] = useState<ProductModeKey | null>(null);

  const selectedModeKey: ProductModeKey = proxyMode === 'tun'
    ? 'whole-computer'
    : systemProxyMode === 'set'
      ? 'browser-apps'
      : 'manual-proxy';
  const activeModeLabel =
    selectedModeKey === 'whole-computer'
      ? t('fullDeviceMode')
      : selectedModeKey === 'browser-apps'
        ? t('browserAppsModeTitle')
        : t('manualProxyModeTitle');

  const switchMode = (mode: ProxyMode, nextSystemProxyMode: SystemProxyMode) => {
    onModeSwitch(mode, nextSystemProxyMode);
  };

  const modeCards: Array<{
    key: ProductModeKey;
    proxyMode: ProxyMode;
    systemProxyMode: SystemProxyMode;
    icon: typeof Globe;
    title: string;
    badge: string;
    body: string;
  }> = [
    {
      key: 'browser-apps',
      proxyMode: 'system-proxy',
      systemProxyMode: 'set',
      icon: Globe,
      title: t('browserAppsModeTitle'),
      badge: t('recommended'),
      body: t('browserAppsModeBody'),
    },
    {
      key: 'whole-computer',
      proxyMode: 'tun',
      systemProxyMode: 'unchanged',
      icon: Network,
      title: t('tunAdvancedTitle'),
      badge: t('fullDeviceMode'),
      body: t('modeHelpTunBody'),
    },
    {
      key: 'manual-proxy',
      proxyMode: 'system-proxy',
      systemProxyMode: 'unchanged',
      icon: SlidersHorizontal,
      title: t('manualProxyModeTitle'),
      badge: t('manualProxyBadge'),
      body: t('manualProxyModeBody'),
    },
  ];

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
              {modeCards.map((item) => {
                const Icon = item.icon;
                const selected = selectedModeKey === item.key;
                return (
                  <div
                    key={item.key}
                    onMouseLeave={() => {
                      if (openHelpMode === item.key) setOpenHelpMode(null);
                    }}
                    className={`relative overflow-visible rounded-xl border-[3px] p-2.5 transition-all duration-300 ${
                      openHelpMode === item.key ? 'z-[90]' : 'hover:z-[70] focus-within:z-[70]'
                    } ${
                      selected
                        ? 'border-black bg-white shadow-[4px_4px_0_#000]'
                        : 'border-black/45 bg-white/60 hover:border-black hover:bg-white'
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <button
                        type="button"
                        onClick={() => switchMode(item.proxyMode, item.systemProxyMode)}
                        disabled={isBusy}
                        className="flex min-w-0 flex-1 cursor-pointer items-start gap-2 text-left disabled:cursor-not-allowed disabled:opacity-60"
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
                        onClick={(event) => {
                          event.stopPropagation();
                          setOpenHelpMode((mode) => (mode === item.key ? null : item.key));
                        }}
                        onMouseEnter={() => setOpenHelpMode(item.key)}
                        onFocus={() => setOpenHelpMode(item.key)}
                        onBlur={() => setOpenHelpMode(null)}
                        aria-label={item.title}
                        aria-expanded={openHelpMode === item.key}
                        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full border-[2px] border-black bg-white text-black transition-all hover:-translate-y-0.5 hover:bg-black hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-black focus-visible:ring-offset-2"
                      >
                        <CircleHelp className="h-3.5 w-3.5 stroke-[3px]" />
                      </button>
                    </div>
                    {openHelpMode === item.key && (
                      <div className="mt-2 w-full rounded-xl border-[2px] border-black bg-white px-3 py-2.5 text-left text-black shadow-[3px_3px_0_rgba(0,0,0,0.18)]">
                        <p className="text-[10px] font-black uppercase tracking-widest">
                          {item.title}
                        </p>
                        <p className="mt-1 text-[10px] font-bold leading-relaxed text-black/65">
                          {item.body}
                        </p>
                      </div>
                    )}
                    {item.key === 'whole-computer' && selected && (
                      <p className="mt-2 rounded-lg border-[2px] border-amber-500 bg-amber-200 px-2 py-1 text-[9px] font-black uppercase tracking-widest text-black">
                        {t('adminPermissionHint')}
                      </p>
                    )}
                    {item.key === 'manual-proxy' && selected && (
                      <p className="mt-2 rounded-lg border-[2px] border-black bg-bg-primary px-2 py-1 text-[9px] font-black uppercase tracking-widest text-black">
                        HTTP 127.0.0.1:{httpPort} · SOCKS5 127.0.0.1:{socksPort}
                      </p>
                    )}
                  </div>
                );
              })}
            </div>

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
