import { useEffect, useState } from 'react';
import { X, MessageCircle, LifeBuoy, ChevronRight } from 'lucide-react';
import { isNetworkExtensionOnlyBuild } from '../../lib/build-policy';

type T = (key: never) => string;

async function open(url: string) {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch {
    window.open(url, '_blank');
  }
}

interface Props {
  onClose: () => void;
  onExportSupportBundle: () => void;
  t: T;
}

/** Design "Help & support" overlay wired to real support channels + bundle export. */
export default function SupportModal({ onClose, onExportSupportBundle, t }: Props) {
  const [ver, setVer] = useState('');
  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(setVer).catch(() => {});
  }, []);

  const rows = [
    {
      icon: MessageCircle,
      title: t('v6SupportChat' as never),
      sub: t('v6SupportChatSub' as never),
      onClick: () => open('https://t.me/doodlevpn_support'),
    },
    ...(!isNetworkExtensionOnlyBuild() ? [{
      icon: LifeBuoy,
      title: t('supportBundle' as never),
      sub: t('v6SupportBundleSub' as never),
      onClick: () => { onExportSupportBundle(); onClose(); },
    }] : []),
  ];

  return (
    <div
      onClick={onClose}
      className="v6-fadein absolute inset-0 z-20 flex items-center justify-center"
      style={{ background: 'rgba(10,5,8,0.5)', backdropFilter: 'blur(8px)', WebkitBackdropFilter: 'blur(8px)' }}
    >
      <div onClick={(e) => e.stopPropagation()} className="v6-modal w-[min(430px,calc(100vw-48px))] rounded-[28px] p-[26px]">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[18px] font-semibold text-white">{t('v6SupportTitle' as never)}</span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('cancel' as never)}
            className="v6-hover-bright flex h-[34px] w-[34px] items-center justify-center rounded-[11px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
          >
            <X className="h-4 w-4" strokeWidth={2.3} />
          </button>
        </div>
        <div className="mb-[18px] text-[13px] text-white/50">{t('v6SupportSub' as never)}</div>

        <div className="flex flex-col gap-2.5">
          {rows.map(({ icon: Icon, title, sub, onClick }) => (
            <button
              key={title}
              type="button"
              onClick={onClick}
              className="v6-glass v6-hover-bright flex w-full items-center gap-3.5 rounded-[18px] px-4 py-[15px] text-left v6-focus"
            >
              <span className="v6-tile-accent flex h-10 w-10 shrink-0 items-center justify-center rounded-xl">
                <Icon className="h-[19px] w-[19px]" strokeWidth={1.9} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-[14.5px] font-medium text-white">{title}</span>
                <span className="mt-0.5 block truncate text-[12px] text-white/45">{sub}</span>
              </span>
              <ChevronRight className="h-4 w-4 shrink-0 text-white/40" strokeWidth={2.2} />
            </button>
          ))}
        </div>

        <div className="mt-[18px] text-center text-[11.5px] text-white/35">DoodleRay{ver ? ` · ${t('v6Version' as never)} ${ver}` : ''}</div>
      </div>
    </div>
  );
}
