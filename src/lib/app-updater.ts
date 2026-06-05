type UpdateEvent = {
  event: 'Started' | 'Progress' | 'Finished';
  data?: {
    contentLength?: number;
    chunkLength?: number;
  };
};

type UpdateLike = {
  version: string;
  download: (callback?: (event: UpdateEvent) => void, options?: { timeout?: number }) => Promise<void>;
  install: () => Promise<void>;
};

type InstallOptions = {
  update?: UpdateLike | null;
  onStatus?: (status: string) => void;
  onProgress?: (progress: number | null) => void;
  disconnectVpn?: boolean;
};

let cachedUpdate: UpdateLike | null = null;
let installPromise: Promise<boolean> | null = null;
const UPDATE_CHECK_TIMEOUT_MS = 15_000;
const UPDATE_DOWNLOAD_TIMEOUT_MS = 5 * 60_000;
const UPDATE_INSTALL_TIMEOUT_MS = 4 * 60_000;
const UPDATE_PREPARE_STEP_TIMEOUT_MS = 8_000;

function timeoutError(label: string, timeoutMs: number) {
  return new Error(`${label} timed out after ${Math.round(timeoutMs / 1000)}s`);
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(timeoutError(label, timeoutMs)), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function getCachedUpdate() {
  return cachedUpdate;
}

export function setCachedUpdate(update: UpdateLike | null) {
  cachedUpdate = update;
}

export async function checkForAppUpdate() {
  const { check } = await import('@tauri-apps/plugin-updater');
  const update = (await check({ timeout: UPDATE_CHECK_TIMEOUT_MS })) as UpdateLike | null;
  cachedUpdate = update;
  return update;
}

async function disconnectBeforeInstall(onStatus: (status: string) => void) {
  onStatus('updateClosingProcesses');

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await withTimeout(invoke('vpn_disconnect').catch(() => {}), UPDATE_PREPARE_STEP_TIMEOUT_MS, 'VPN disconnect');
    await withTimeout(invoke('prepare_for_app_update'), UPDATE_PREPARE_STEP_TIMEOUT_MS, 'Update preparation');
    await withTimeout(invoke('vpn_disconnect').catch(() => {}), UPDATE_PREPARE_STEP_TIMEOUT_MS, 'Final VPN disconnect');
    await new Promise((resolve) => setTimeout(resolve, 1500));
  } catch (e) {
    console.warn('Could not disconnect VPN before update:', e);
  }
}

export async function installAppUpdate({
  update,
  onStatus = () => {},
  onProgress = () => {},
  disconnectVpn = true,
}: InstallOptions = {}) {
  if (installPromise) return installPromise;

  installPromise = installAppUpdateOnce({ update, onStatus, onProgress, disconnectVpn }).finally(() => {
    installPromise = null;
  });

  return installPromise;
}

async function installAppUpdateOnce({
  update,
  onStatus = () => {},
  onProgress = () => {},
  disconnectVpn = true,
}: InstallOptions = {}) {
  const pendingUpdate = update ?? cachedUpdate ?? await checkForAppUpdate();

  if (!pendingUpdate) {
    onStatus('updateLatest');
    onProgress(null);
    return false;
  }

  onStatus('updateDownloading');
  onProgress(0);

  let downloaded = 0;
  let contentLength = 0;

  await pendingUpdate.download((event) => {
    switch (event.event) {
      case 'Started':
        contentLength = event.data?.contentLength || 0;
        onProgress(0);
        break;
      case 'Progress':
        downloaded += event.data?.chunkLength || 0;
        if (contentLength > 0) {
          const percent = Math.min(100, Math.round((downloaded / contentLength) * 100));
          onProgress(percent);
          onStatus('updateDownloadingProgress');
        }
        break;
      case 'Finished':
        onProgress(100);
        onStatus('updatePreparingInstall');
        break;
    }
  }, { timeout: UPDATE_DOWNLOAD_TIMEOUT_MS });

  if (disconnectVpn) {
    await disconnectBeforeInstall(onStatus);
  }

  onStatus('updateInstallingRestarting');
  await withTimeout(pendingUpdate.install(), UPDATE_INSTALL_TIMEOUT_MS, 'Update installation');
  cachedUpdate = null;

  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
  return true;
}
