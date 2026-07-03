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

interface Props {
  /** Accent color for the brand status dot (protected/degraded/etc.) */
  statusColor?: string;
  statusLabel?: string;
}

/**
 * Custom window chrome for the v6 dark-glass shell. The window is undecorated
 * (tauri.conf `decorations: false`); Tauri keeps native edge-resize on Windows,
 * so we only own drag + minimize/maximize/close. `data-tauri-drag-region` moves
 * the window; interactive controls opt out via their own handlers.
 */
export default function TitleBar({ statusColor, statusLabel }: Props) {
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
    <div
      data-tauri-drag-region
      className="relative z-[200] flex h-9 shrink-0 items-center justify-between pl-3 pr-1 select-none"
    >
      <div data-tauri-drag-region className="flex items-center gap-2.5">
        <span
          data-tauri-drag-region
          className="h-4 w-4 rounded-[6px] bg-gradient-to-br from-[#7c6cff] to-[#3dd7c8] shadow-[0_0_12px_rgba(124,108,255,0.6)]"
          aria-hidden
        />
        <span data-tauri-drag-region className="text-[12px] font-semibold tracking-tight text-v6-text/90">
          DoodleRay
        </span>
        {statusColor && (
          <span data-tauri-drag-region className="ml-1 flex items-center gap-1.5">
            <span
              className="h-1.5 w-1.5 rounded-full"
              style={{ background: statusColor, boxShadow: `0 0 8px ${statusColor}` }}
            />
            {statusLabel && (
              <span className="text-[10px] font-medium uppercase tracking-wider text-v6-muted">{statusLabel}</span>
            )}
          </span>
        )}
      </div>

      <div className="flex items-center gap-0.5">
        <TitleButton label="Minimize" onClick={() => windowAction('minimize')}>
          <Minus className="h-3.5 w-3.5" strokeWidth={2.4} />
        </TitleButton>
        <TitleButton label={maximized ? 'Restore' : 'Maximize'} onClick={() => windowAction('toggleMaximize')}>
          {maximized ? <Copy className="h-3 w-3" strokeWidth={2.4} /> : <Square className="h-3 w-3" strokeWidth={2.4} />}
        </TitleButton>
        <TitleButton label="Close" danger onClick={() => windowAction('close')}>
          <X className="h-4 w-4" strokeWidth={2.4} />
        </TitleButton>
      </div>
    </div>
  );
}

function TitleButton({
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
      className={`flex h-7 w-10 items-center justify-center rounded-md text-v6-muted transition-colors v6-focus hover:text-v6-text ${
        danger ? 'hover:bg-[#f8717133] hover:text-[#fca5a5]' : 'hover:bg-white/10'
      }`}
    >
      {children}
    </button>
  );
}
