import type { ReactNode } from 'react';
import { ArrowDown, ArrowUp, Clock, Network } from 'lucide-react';
import { formatSpeed, formatBytes, formatDuration } from '../../lib/utils';
import type { ProxyMode, SystemProxyMode } from '../../stores/app-store';

type T = (key: never) => string;

interface Props {
  connected: boolean;
  connectTime: number;
  currentDownload: number;
  currentUpload: number;
  totalDown: number;
  totalUp: number;
  socksPort: number;
  httpPort: number;
  proxyMode: ProxyMode;
  systemProxyMode: SystemProxyMode;
  t: T;
}

/** Live throughput + session totals + local proxy ports. */
export default function TrafficStats({
  connected,
  connectTime,
  currentDownload,
  currentUpload,
  totalDown,
  totalUp,
  socksPort,
  httpPort,
  proxyMode,
  systemProxyMode,
  t,
}: Props) {
  const showPorts = proxyMode === 'tun' || systemProxyMode !== 'unchanged' || connected;
  return (
    <div className="v6-glass rounded-xl p-3">
      <div className="mb-2.5 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-v6-muted">
        <Network className="h-3.5 w-3.5" strokeWidth={2.2} />
        {t('liveThroughput' as never)}
      </div>

      <div className="grid grid-cols-2 gap-2">
        <Stat
          icon={<ArrowDown className="h-3.5 w-3.5" strokeWidth={2.4} />}
          color="#34d399"
          label={t('download' as never)}
          value={connected ? formatSpeed(currentDownload) : '—'}
          sub={formatBytes(totalDown)}
        />
        <Stat
          icon={<ArrowUp className="h-3.5 w-3.5" strokeWidth={2.4} />}
          color="#7c6cff"
          label={t('upload' as never)}
          value={connected ? formatSpeed(currentUpload) : '—'}
          sub={formatBytes(totalUp)}
        />
      </div>

      <div className="mt-2 flex items-center justify-between border-t border-v6-line pt-2 text-[10.5px] text-v6-muted">
        <span className="flex items-center gap-1 tabular-nums">
          <Clock className="h-3 w-3" strokeWidth={2.2} />
          {connected ? formatDuration(connectTime) : '00:00:00'}
        </span>
        {showPorts && (
          <span className="tabular-nums">
            SOCKS {socksPort} · HTTP {httpPort}
          </span>
        )}
      </div>
    </div>
  );
}

function Stat({
  icon,
  color,
  label,
  value,
  sub,
}: {
  icon: ReactNode;
  color: string;
  label: string;
  value: string;
  sub: string;
}) {
  return (
    <div className="v6-glass-inset rounded-lg p-2.5">
      <div className="flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-wider text-v6-muted">
        <span style={{ color }}>{icon}</span>
        {label}
      </div>
      <div className="mt-1 truncate text-[15px] font-semibold tabular-nums text-v6-text" style={{ color }}>
        {value}
      </div>
      <div className="truncate text-[10px] tabular-nums text-v6-muted">{sub}</div>
    </div>
  );
}
