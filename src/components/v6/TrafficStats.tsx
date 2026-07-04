import { ArrowDown, ArrowUp } from 'lucide-react';

type T = (key: never) => string;

function mbps(bytesPerSec: number): string {
  return ((bytesPerSec * 8) / 1_000_000).toFixed(1);
}

interface Props {
  connected: boolean;
  currentDownload: number;
  currentUpload: number;
  t: T;
}

/** Design stat cards: Down / Up in Mb/s (live, real traffic poll). */
export default function TrafficStats({ connected, currentDownload, currentUpload, t }: Props) {
  const cards = [
    { label: t('download' as never), value: connected ? mbps(currentDownload) : '0.0', color: 'currentColor', Icon: ArrowDown, iconColor: 'rgba(255,255,255,0.5)' },
    { label: t('upload' as never), value: connected ? mbps(currentUpload) : '0.0', color: 'currentColor', Icon: ArrowUp, iconColor: '#FF8A4C' },
  ];
  return (
    <div className="flex gap-3.5">
      {cards.map(({ label, value, Icon, iconColor }) => (
        <div key={label} className="w-[118px] rounded-[20px] border border-white/[0.09] bg-white/[0.05] px-4 py-3.5">
          <div className="mb-[7px] flex items-center gap-1.5 text-white/50">
            <Icon className="h-[13px] w-[13px]" style={{ color: iconColor }} strokeWidth={2.2} />
            <span className="text-[11px]">{label}</span>
          </div>
          <div className="text-white">
            <span className="text-[21px] font-semibold tabular-nums">{value}</span>{' '}
            <span className="text-[11px] text-white/50">Mb/s</span>
          </div>
        </div>
      ))}
    </div>
  );
}
