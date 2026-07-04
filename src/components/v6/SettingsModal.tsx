import { useState } from 'react';
import { X, LogOut, RefreshCw, Trash2, DownloadCloud, Wrench, Loader2 } from 'lucide-react';
import { useAppStore, type SupportedLanguage } from '../../stores/app-store';
import { refreshSubscription } from '../../lib/subscription';
import Toggle from './Toggle';

type T = (key: never) => string;

function isTauri() {
  return typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';
}

function SectionTitle({ children }: { children: string }) {
  return <div className="px-1 pb-1 pt-4 text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{children}</div>;
}

function Row({ title, sub, right, onClick, danger }: {
  title: string; sub?: string; right: React.ReactNode; onClick?: () => void; danger?: boolean;
}) {
  const Tag = onClick ? 'button' : 'div';
  return (
    <Tag
      type={onClick ? 'button' : undefined}
      onClick={onClick}
      className={`flex w-full items-center gap-3 border-b border-white/[0.07] px-1 py-3 text-left ${onClick ? 'v6-focus' : ''}`}
    >
      <span className="min-w-0 flex-1">
        <span className={`block text-[14px] font-medium ${danger ? 'text-[#ff9a8c]' : 'text-white'}`}>{title}</span>
        {sub && <span className="mt-0.5 block text-[11.5px] leading-snug text-white/45">{sub}</span>}
      </span>
      {right}
    </Tag>
  );
}

function PortInput({ value, onCommit, label }: { value: number; onCommit: (v: number) => void; label: string }) {
  const [draft, setDraft] = useState(String(value));
  return (
    <input
      type="text"
      inputMode="numeric"
      value={draft}
      aria-label={label}
      onChange={(e) => setDraft(e.target.value.replace(/\D/g, '').slice(0, 5))}
      onBlur={() => {
        const n = parseInt(draft, 10);
        if (Number.isFinite(n) && n >= 1024 && n <= 65535) onCommit(n);
        else setDraft(String(value));
      }}
      className="v6-glass-inset w-[86px] rounded-xl px-3 py-2 text-center text-[13.5px] font-medium tabular-nums text-white outline-none v6-focus"
    />
  );
}

/** Full v6 settings overlay — every control drives real store/backend actions. */
export default function SettingsModal({ onClose, t }: { onClose: () => void; t: T }) {
  const {
    autoStart, silentAdminAutostart,
    autoConnectOnStartup, setAutoConnectOnStartup,
    killSwitch, setKillSwitch,
    showStats, setShowStats,
    language, setLanguage,
    socksPort, setSocksPort, httpPort, setHttpPort,
    strictRoute, setStrictRoute,
    autoSelectFastest, setAutoSelectFastest,
    subAutoUpdateMinutes, setSubAutoUpdateMinutes,
    subscriptions, updateSubscription, removeSubscription,
    wipeData, addLog,
  } = useAppStore();

  const [refreshingId, setRefreshingId] = useState<string | null>(null);
  const [armedDeleteId, setArmedDeleteId] = useState<string | null>(null);
  const [updateStatus, setUpdateStatus] = useState<string | null>(null);
  const [repairing, setRepairing] = useState(false);
  const [wipeArmed, setWipeArmed] = useState(false);

  const launchOn = autoStart || silentAdminAutostart;
  const toggleLaunch = async () => {
    if (silentAdminAutostart) return; // managed by admin autostart; leave as-is
    const next = !autoStart;
    useAppStore.setState({ autoStart: next });
    try {
      const { enable, disable } = await import('@tauri-apps/plugin-autostart');
      if (next) await enable(); else await disable();
    } catch {
      useAppStore.setState({ autoStart: !next });
    }
  };

  const handleRefreshSub = async (subId: string) => {
    const sub = subscriptions.find((s) => s.id === subId);
    if (!sub || refreshingId) return;
    setRefreshingId(subId);
    try {
      updateSubscription(subId, await refreshSubscription(sub));
      addLog('success', `Subscription refreshed: ${sub.name}`);
    } catch (err) {
      addLog('error', `Subscription refresh failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setRefreshingId(null);
    }
  };

  const handleCheckUpdates = async () => {
    setUpdateStatus(t('updateChecking' as never));
    try {
      const { checkForAppUpdate } = await import('../../lib/app-updater');
      const update = await checkForAppUpdate();
      if (update) {
        useAppStore.getState().setUpdateState({
          availableUpdate: update.version,
          updatePhase: 'available',
          updateStatus: '',
          updateProgress: null,
        });
        setUpdateStatus(`v${update.version} →`);
      } else {
        setUpdateStatus(t('updateLatest' as never));
        setTimeout(() => setUpdateStatus(null), 4000);
      }
    } catch (err) {
      setUpdateStatus(err instanceof Error ? err.message : String(err));
      setTimeout(() => setUpdateStatus(null), 5000);
    }
  };

  const handleRepair = async () => {
    if (!isTauri() || repairing) return;
    setRepairing(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const message = await invoke('repair_windows_runtime') as string;
      addLog('info', message.split('\n')[0]);
      const { useToastStore } = await import('../../stores/toast-store');
      useToastStore.getState().addToast(message.split('\n')[0], 'success');
    } catch (err) {
      addLog('error', `Repair failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setRepairing(false);
    }
  };

  return (
    <div
      onClick={onClose}
      className="v6-fadein absolute inset-0 z-20 flex items-center justify-center"
      style={{ background: 'rgba(10,5,8,0.5)', backdropFilter: 'blur(8px)', WebkitBackdropFilter: 'blur(8px)' }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="v6-modal flex max-h-[calc(100vh-88px)] w-[min(460px,calc(100vw-48px))] flex-col rounded-[28px] p-[26px] pt-[22px]"
      >
        <div className="mb-1 flex shrink-0 items-center justify-between">
          <span className="text-[18px] font-semibold text-white">{t('settings' as never)}</span>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('cancel' as never)}
            className="v6-hover-bright flex h-[34px] w-[34px] items-center justify-center rounded-[11px] border border-white/[0.12] bg-white/[0.08] text-white/70 v6-focus"
          >
            <X className="h-4 w-4" strokeWidth={2.3} />
          </button>
        </div>

        <div className="-mr-3 min-h-0 flex-1 overflow-y-auto pr-3">
          {/* General */}
          <Row title={t('v6SetLaunch' as never)} sub={t('v6SetLaunchSub' as never)} onClick={toggleLaunch} right={<Toggle on={launchOn} label={t('v6SetLaunch' as never)} />} />
          <Row title={t('v6SetAutoConnect' as never)} sub={t('v6SetAutoConnectSub' as never)} onClick={() => setAutoConnectOnStartup(!autoConnectOnStartup)} right={<Toggle on={autoConnectOnStartup} label={t('v6SetAutoConnect' as never)} />} />
          <Row title={t('v6SetKillSwitch' as never)} sub={t('v6SetKillSwitchSub' as never)} onClick={() => setKillSwitch(!killSwitch)} right={<Toggle on={killSwitch} label={t('v6SetKillSwitch' as never)} />} />
          <Row title={t('v6SetStats' as never)} sub={t('v6SetStatsSub' as never)} onClick={() => setShowStats(!showStats)} right={<Toggle on={showStats} label={t('v6SetStats' as never)} />} />
          <Row
            title={t('v6SetLanguage' as never)}
            sub={t('v6SetLanguageSub' as never)}
            right={
              <select
                value={language}
                onChange={(e) => setLanguage(e.target.value as SupportedLanguage)}
                className="cursor-pointer rounded-xl border border-white/[0.14] bg-white/[0.08] px-3.5 py-2 text-[13.5px] font-medium text-white outline-none v6-focus [&>option]:bg-[#1c1116]"
              >
                <option value="en">English</option>
                <option value="ru">Русский</option>
                <option value="zh">中文</option>
              </select>
            }
          />

          {/* Connection */}
          <SectionTitle>{t('v6SecConnection' as never)}</SectionTitle>
          <Row title={t('v6SetAutoSelect' as never)} sub={t('v6SetAutoSelectSub' as never)} onClick={() => setAutoSelectFastest(!autoSelectFastest)} right={<Toggle on={autoSelectFastest} label={t('v6SetAutoSelect' as never)} />} />
          <Row title={t('strictRoute' as never)} sub={t('strictRouteDesc' as never)} onClick={() => setStrictRoute(!strictRoute)} right={<Toggle on={strictRoute} label={t('strictRoute' as never)} />} />
          <Row title={t('socksPort' as never)} right={<PortInput value={socksPort} onCommit={setSocksPort} label={t('socksPort' as never)} />} />
          <Row title={t('httpPort' as never)} right={<PortInput value={httpPort} onCommit={setHttpPort} label={t('httpPort' as never)} />} />
          <Row
            title={t('subAutoUpdate' as never)}
            sub={t('subAutoUpdateDesc' as never)}
            right={
              <select
                value={subAutoUpdateMinutes}
                onChange={(e) => setSubAutoUpdateMinutes(Number(e.target.value))}
                className="cursor-pointer rounded-xl border border-white/[0.14] bg-white/[0.08] px-3 py-2 text-[13px] font-medium text-white outline-none v6-focus [&>option]:bg-[#1c1116]"
              >
                <option value={0}>—</option>
                <option value={60}>1h</option>
                <option value={180}>3h</option>
                <option value={720}>12h</option>
                <option value={1440}>24h</option>
              </select>
            }
          />

          {/* Subscriptions */}
          {subscriptions.length > 0 && (
            <>
              <SectionTitle>{t('subscriptions' as never)}</SectionTitle>
              {subscriptions.map((sub) => (
                <div key={sub.id} className="flex items-center gap-2 border-b border-white/[0.07] px-1 py-3">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[14px] font-medium text-white" title={sub.name}>{sub.name}</span>
                    <span className="mt-0.5 block text-[11px] text-white/40">{new Date(sub.updatedAt).toLocaleString()}</span>
                  </span>
                  <button
                    type="button"
                    onClick={() => handleRefreshSub(sub.id)}
                    disabled={!!refreshingId}
                    title={t('refreshSub' as never)}
                    aria-label={t('refreshSub' as never)}
                    className="v6-hover-bright grid h-8 w-8 place-items-center rounded-[10px] border border-white/[0.1] bg-white/[0.06] text-white/70 v6-focus disabled:opacity-40"
                  >
                    <RefreshCw className={`h-3.5 w-3.5 ${refreshingId === sub.id ? 'v6-orb-spin' : ''}`} strokeWidth={2.2} />
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      if (armedDeleteId === sub.id) { removeSubscription(sub.id); setArmedDeleteId(null); }
                      else setArmedDeleteId(sub.id);
                    }}
                    title={t('deleteSub' as never)}
                    aria-label={t('deleteSub' as never)}
                    className={`grid h-8 place-items-center rounded-[10px] border px-2 text-[11px] font-medium transition-colors v6-focus ${
                      armedDeleteId === sub.id
                        ? 'border-[#ff6b5a]/50 bg-[#ff6b5a]/20 text-[#ffb3a8]'
                        : 'v6-hover-bright border-white/[0.1] bg-white/[0.06] text-white/70'
                    }`}
                  >
                    {armedDeleteId === sub.id ? t('v6ConfirmAgain' as never) : <Trash2 className="h-3.5 w-3.5" strokeWidth={2.2} />}
                  </button>
                </div>
              ))}
            </>
          )}

          {/* Maintenance */}
          <SectionTitle>{t('v6SecMaintenance' as never)}</SectionTitle>
          <Row
            title={t('checkForUpdates' as never)}
            sub={updateStatus ?? undefined}
            onClick={handleCheckUpdates}
            right={<DownloadCloud className="h-[18px] w-[18px] shrink-0 text-white/50" strokeWidth={1.9} />}
          />
          <Row
            title={t('v6SetRepair' as never)}
            sub={t('v6SetRepairSub' as never)}
            onClick={handleRepair}
            right={repairing
              ? <Loader2 className="h-[18px] w-[18px] shrink-0 v6-orb-spin text-[#FF8A4C]" strokeWidth={2} />
              : <Wrench className="h-[18px] w-[18px] shrink-0 text-white/50" strokeWidth={1.9} />}
          />
          <Row
            title={wipeArmed ? t('v6ConfirmAgain' as never) : t('v6SetWipe' as never)}
            sub={t('v6SetWipeSub' as never)}
            danger
            onClick={() => {
              if (wipeArmed) { wipeData(); setWipeArmed(false); addLog('warning', 'All servers and subscriptions were removed'); }
              else { setWipeArmed(true); setTimeout(() => setWipeArmed(false), 4000); }
            }}
            right={<Trash2 className="h-[18px] w-[18px] shrink-0 text-[#ff8a7a]" strokeWidth={1.9} />}
          />

          {/* Quit */}
          <Row
            title={t('quit' as never)}
            danger
            onClick={async () => {
              try {
                const { invoke } = await import('@tauri-apps/api/core');
                await invoke('vpn_disconnect').catch(() => {});
                await invoke('quit_app');
              } catch {
                window.close();
              }
            }}
            right={<LogOut className="h-[18px] w-[18px] shrink-0 text-[#ff8a7a]" strokeWidth={1.9} />}
          />
        </div>
      </div>
    </div>
  );
}
