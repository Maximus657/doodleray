import { NavLink } from 'react-router-dom';
import { useEffect, useState, type ReactNode, type CSSProperties } from 'react';
import { Home, Server, Clock, Settings, LogOut, HelpCircle } from 'lucide-react';
import { useTranslation } from '../../locales';

const NAV_ITEMS = [
  { path: '/', icon: Home, labelKey: 'dashboard' },
  { path: '/servers', icon: Server, labelKey: 'servers' },
  { path: '/workshop', icon: Clock, labelKey: 'workshop' },
  { path: '/settings', icon: Settings, labelKey: 'settings' },
] as const;

/** Glass navigation rail for the v6 shell. Same routes/actions as the legacy Sidebar. */
export default function NavRail() {
  const { t } = useTranslation();
  const [ver, setVer] = useState('');

  useEffect(() => {
    import('@tauri-apps/api/app')
      .then(({ getVersion }) => getVersion())
      .then((v) => setVer(`v${v}`))
      .catch(() => {});
  }, []);

  return (
    <aside className="relative z-[90] flex h-full w-[72px] shrink-0 flex-col items-center py-3">
      <nav className="flex w-full flex-1 flex-col items-center gap-2 pt-1">
        {NAV_ITEMS.map(({ path, icon: Icon, labelKey }) => (
          <NavLink
            key={path}
            to={path}
            aria-label={t(labelKey)}
            title={t(labelKey)}
            className={({ isActive }) =>
              `group relative flex h-12 w-12 items-center justify-center rounded-2xl transition-all duration-200 v6-focus ${
                isActive
                  ? 'bg-white/10 text-v6-text shadow-[inset_0_1px_0_rgba(255,255,255,0.12)]'
                  : 'text-v6-muted hover:bg-white/[0.06] hover:text-v6-text'
              }`
            }
          >
            {({ isActive }) => (
              <>
                {isActive && (
                  <span className="absolute -left-2 h-6 w-[3px] rounded-full bg-gradient-to-b from-[#7c6cff] to-[#3dd7c8]" />
                )}
                <Icon className="h-[22px] w-[22px]" strokeWidth={2.1} />
                <Tooltip>{t(labelKey)}</Tooltip>
              </>
            )}
          </NavLink>
        ))}
      </nav>

      <div className="flex flex-col items-center gap-2 pb-1">
        <RailButton
          label={t('support' as never) || 'Support'}
          accent="#8b5cf6"
          onClick={async () => {
            try {
              const { openUrl } = await import('@tauri-apps/plugin-opener');
              await openUrl('https://t.me/doodlevpn_support');
            } catch (e) {
              console.error(e);
            }
          }}
        >
          <HelpCircle className="h-[21px] w-[21px]" strokeWidth={2.1} />
        </RailButton>
        <RailButton
          label={t('quit')}
          accent="#f87171"
          onClick={async () => {
            try {
              const { invoke } = await import('@tauri-apps/api/core');
              await invoke('vpn_disconnect').catch(() => {});
              await invoke('quit_app');
            } catch {
              window.close();
            }
          }}
        >
          <LogOut className="h-[21px] w-[21px]" strokeWidth={2.1} />
        </RailButton>
        {ver && <div className="mt-1 text-[9px] font-medium tracking-wider text-v6-muted/50">{ver}</div>}
      </div>
    </aside>
  );
}

function RailButton({
  children,
  onClick,
  label,
  accent,
}: {
  children: ReactNode;
  onClick: () => void;
  label: string;
  accent: string;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="group relative flex h-12 w-12 items-center justify-center rounded-2xl text-v6-muted transition-all duration-200 hover:bg-white/[0.06] v6-focus"
      style={{ '--accent': accent } as CSSProperties}
    >
      <span className="transition-colors group-hover:text-[var(--accent)]">{children}</span>
      <Tooltip>{label}</Tooltip>
    </button>
  );
}

function Tooltip({ children }: { children: React.ReactNode }) {
  return (
    <span className="pointer-events-none absolute left-full z-[120] ml-2 whitespace-nowrap rounded-lg border border-v6-line bg-[#11151f] px-2.5 py-1.5 text-[11px] font-medium text-v6-text opacity-0 shadow-xl transition-all duration-150 group-hover:translate-x-0 group-hover:opacity-100 -translate-x-1">
      {children}
    </span>
  );
}
