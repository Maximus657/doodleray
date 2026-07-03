import type { ReactNode } from 'react';
import { useAppStore } from '../../stores/app-store';
import { useTranslation } from '../../locales';
import TitleBar from './TitleBar';
import NavRail from './NavRail';
import { deriveOrbState, ORB_COLORS, ORB_LABEL_KEY } from './status';

/**
 * v6 shell: custom titlebar + glass nav rail + routed content. Renders the
 * whole app on a dark-glass surface. Reads only the coarse connection status
 * for the titlebar dot; the dashboard owns the detailed health verdict.
 */
export default function AppShell({ children }: { children: ReactNode }) {
  const status = useAppStore((s) => s.status);
  const productMode = useAppStore((s) => s.productMode);
  const { t } = useTranslation();

  const orb = deriveOrbState(status, productMode);
  const statusLabel = t(ORB_LABEL_KEY[orb] as never);

  return (
    <div className="v6-app flex h-screen w-screen flex-col overflow-hidden">
      <TitleBar statusColor={ORB_COLORS[orb]} statusLabel={statusLabel} />
      <div className="flex min-h-0 flex-1 px-2 pb-2">
        <NavRail />
        <main className="v6-glass relative flex min-w-0 flex-1 flex-col overflow-hidden rounded-2xl">
          {children}
        </main>
      </div>
    </div>
  );
}
