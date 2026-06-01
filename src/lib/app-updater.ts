type UpdateEvent = {
  event: 'Started' | 'Progress' | 'Finished';
  data?: {
    contentLength?: number;
    chunkLength?: number;
  };
};

type UpdateLike = {
  version: string;
  download: (callback?: (event: UpdateEvent) => void) => Promise<void>;
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

export function getCachedUpdate() {
  return cachedUpdate;
}

export function setCachedUpdate(update: UpdateLike | null) {
  cachedUpdate = update;
}

export async function checkForAppUpdate() {
  const { check } = await import('@tauri-apps/plugin-updater');
  const update = (await check()) as UpdateLike | null;
  cachedUpdate = update;
  return update;
}

async function disconnectBeforeInstall(onStatus: (status: string) => void) {
  onStatus('Closing background processes...');

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('vpn_disconnect');
    await new Promise((resolve) => setTimeout(resolve, 1000));
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
    onStatus('You are on the latest version');
    onProgress(null);
    return false;
  }

  onStatus(`v${pendingUpdate.version} available. Downloading...`);
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
          onStatus(`Downloading... ${percent}%`);
        }
        break;
      case 'Finished':
        onProgress(100);
        onStatus('Download complete. Preparing install...');
        break;
    }
  });

  if (disconnectVpn) {
    await disconnectBeforeInstall(onStatus);
  }

  onStatus('Installing and restarting...');
  await pendingUpdate.install();
  cachedUpdate = null;

  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
  return true;
}
