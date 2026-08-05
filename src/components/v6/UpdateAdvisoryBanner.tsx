import { Download, Loader2 } from 'lucide-react';
import { useTranslation } from '../../locales';
import { installAppUpdate, updatePhaseFromStatus } from '../../lib/app-updater';
import { isInAppUpdateEnabled, openStoreUpdatePage } from '../../lib/update-channel';
import { useAppStore } from '../../stores/app-store';
import { useToastStore } from '../../stores/toast-store';

function formatMessage(template: string, values: Record<string, string | number>) {
  return Object.entries(values).reduce(
    (message, [key, value]) => message.replace(new RegExp(`\\{${key}\\}`, 'g'), String(value)),
    template,
  );
}

function updateStatusLabel(
  status: string,
  phase: string,
  progress: number | null,
  version: string,
  t: (key: any) => string,
) {
  if (progress !== null && phase === 'downloading') {
    return formatMessage(t('updateDownloadingProgress'), { progress });
  }

  switch (status) {
    case 'updateChecking':
      return t('updateChecking');
    case 'updateDownloading':
    case 'updateDownloadingProgress':
      return formatMessage(t('updateDownloadingVersion'), { version });
    case 'updatePreparingInstall':
      return t('updatePreparingInstall');
    case 'updateClosingProcesses':
      return t('updateClosingProcesses');
    case 'updateInstallingRestarting':
      return t('updateInstallingRestarting');
    case 'updateOpenStore':
      return t('updateOpenStore');
    case 'updateLatest':
      return t('updateLatest');
    default:
      return status;
  }
}

export default function UpdateAdvisoryBanner() {
  const { t } = useTranslation();
  const availableUpdate = useAppStore((s) => s.availableUpdate);
  const backendUpdateMinimumVersion = useAppStore((s) => s.backendUpdateMinimumVersion);
  const updatePhase = useAppStore((s) => s.updatePhase);
  const updateStatus = useAppStore((s) => s.updateStatus);
  const updateProgress = useAppStore((s) => s.updateProgress);
  const setUpdateState = useAppStore((s) => s.setUpdateState);

  const updateVersion = availableUpdate ?? backendUpdateMinimumVersion;
  const isBackendAdvisory = !availableUpdate && !!backendUpdateMinimumVersion;
  if (!updateVersion) return null;

  const isDownloading = updatePhase === 'downloading';
  const isBusy = updatePhase === 'checking' || updatePhase === 'downloading' || updatePhase === 'installing';
  const detail = isBusy || updatePhase === 'error'
    ? updateStatusLabel(updateStatus, updatePhase, updateProgress, updateVersion, t) || t('updating')
    : isBackendAdvisory
      ? formatMessage(t('updateAdvisoryBody'), { version: updateVersion })
      : t('updateAvailableBody');
  const actionLabel = isBusy
    ? (isDownloading && updateProgress !== null ? `${t('updateDownloading')} ${updateProgress}%` : t('updating'))
    : (isInAppUpdateEnabled() ? t('installRestart') : t('updateOpenStore'));

  const handleInstall = async () => {
    if (isBusy) return;
    if (!isInAppUpdateEnabled()) {
      await openStoreUpdatePage();
      setUpdateState({ updatePhase: 'available', updateStatus: 'updateOpenStore', updateProgress: null });
      return;
    }

    setUpdateState({ updatePhase: 'downloading', updateStatus: 'updateDownloading', updateProgress: 0 });
    try {
      const updated = await installAppUpdate({
        onStatus: (status) => setUpdateState({ updateStatus: status, updatePhase: updatePhaseFromStatus(status) }),
        onProgress: (progress) => setUpdateState({ updateProgress: progress }),
      });
      if (!updated && isBackendAdvisory) {
        setUpdateState({ updatePhase: 'error', updateStatus: t('updateAdvisoryNotPublished'), updateProgress: null });
      }
    } catch (error) {
      console.error('Update failed:', error);
      setUpdateState({ updatePhase: 'error', updateStatus: t('updateFailed'), updateProgress: null });
      useToastStore.getState().addToast(t('updateFailed'), 'error');
    }
  };

  return (
    <section
      aria-live="polite"
      className="mb-4 flex shrink-0 flex-wrap items-center gap-3 rounded-[18px] border border-[#ffb454]/45 bg-[linear-gradient(105deg,rgba(97,58,30,0.96),rgba(65,45,41,0.96))] px-4 py-3 text-white shadow-[0_10px_28px_rgba(99,57,20,0.2)]"
    >
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-[#ffd195]/30 bg-[#ffad42]/15 text-[#ffd195]">
        {isBusy ? <Loader2 className="h-4.5 w-4.5 animate-spin" strokeWidth={2.4} /> : <Download className="h-4.5 w-4.5" strokeWidth={2.4} />}
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-semibold text-white">
          {isBackendAdvisory ? t('updateAdvisoryTitle') : t('newUpdate')} <span className="text-[#ffd195]">v{updateVersion}</span>
        </p>
        <p className="mt-0.5 text-[11.5px] leading-relaxed text-white/70">{detail}</p>
      </div>
      <button
        type="button"
        onClick={handleInstall}
        disabled={isBusy}
        className="v6-focus flex shrink-0 items-center justify-center gap-1.5 rounded-xl border border-[#ffd195]/35 bg-[#ffb454] px-3 py-2 text-[11.5px] font-semibold text-[#372313] transition-colors hover:bg-[#ffc573] disabled:cursor-wait disabled:opacity-75"
      >
        {isBusy && <Loader2 className="h-3.5 w-3.5 animate-spin" strokeWidth={2.4} />}
        {actionLabel}
      </button>
    </section>
  );
}
