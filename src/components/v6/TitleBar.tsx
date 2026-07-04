import { useEffect, useState, type ReactNode } from 'react';
import { Minus, Square, X, Copy } from 'lucide-react';

function isTauri() {
  return typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';
}

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!isTauri()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const w = getCurrentWindow();
    if (action === 'minimize') await w.minimize();
    else if (action === 'toggleMaximize') await w.toggleMaximize();
    else await w.close();
  } catch {
    /* window controls unavailable (dev/browser) */
  }
}

/**
 * Windows window controls for the undecorated v6 window, styled as the
 * design's glass icon buttons. Native edge-resize stays available; the drag
 * region is owned by the AppShell header.
 */
export default function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const w = getCurrentWindow();
        setMaximized(await w.isMaximized());
        unlisten = await w.onResized(async () => setMaximized(await w.isMaximized()));
      } catch {
        /* ignore */
      }
    })();
    return () => unlisten?.();
  }, []);

  return (
    <div className="ml-1 flex items-center gap-1.5 border-l border-white/10 pl-2.5">
      <ControlButton label="Minimize" onClick={() => windowAction('minimize')}>
        <Minus className="h-[15px] w-[15px]" strokeWidth={2.2} />
      </ControlButton>
      <ControlButton label={maximized ? 'Restore' : 'Maximize'} onClick={() => windowAction('toggleMaximize')}>
        {maximized ? <Copy className="h-3 w-3" strokeWidth={2.2} /> : <Square className="h-3 w-3" strokeWidth={2.2} />}
      </ControlButton>
      <ControlButton label="Close" danger onClick={() => windowAction('close')}>
        <X className="h-4 w-4" strokeWidth={2.2} />
      </ControlButton>
    </div>
  );
}

function ControlButton({
  children,
  onClick,
  label,
  danger,
}: {
  children: ReactNode;
  onClick: () => void;
  label: string;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-8 w-9 items-center justify-center rounded-[11px] border border-white/[0.10] bg-white/[0.05] text-white/70 transition-colors v6-focus ${
        danger ? 'hover:border-[#ff6b5a]/50 hover:bg-[#ff6b5a]/25 hover:text-white' : 'hover:bg-white/[0.14]'
      }`}
    >
      {children}
    </button>
  );
}
