import { useState, useEffect } from 'react';
import { Settings as SettingsIcon, Trash2, RotateCcw, Database, Zap, Monitor, Download, ShieldCheck } from 'lucide-react';
import { disable } from '@tauri-apps/plugin-autostart';
import { useTranslation } from '../locales';
import { useAppStore } from '../stores/app-store';

function Toggle({ checked, onChange, label, description, warning }: { checked: boolean; onChange: (v: boolean) => void; label: string; description?: string; warning?: string }) {
  return (
    <label className="flex items-center justify-between cursor-pointer py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] hover:translate-x-[-1px] hover:translate-y-[-1px] hover:shadow-[4px_4px_0_#000] transition-all rounded-xl">
      <div className="flex-1 min-w-0 mr-3">
        <span className="text-sm font-black text-black block uppercase tracking-tight">{label}</span>
        {description && <span className="text-[10px] font-black text-black/60 block mt-0.5 tracking-widest uppercase">{description}</span>}
        {warning && !checked && <span className="text-[10px] text-red-600 font-black block mt-1 tracking-widest uppercase">{warning}</span>}
      </div>
      <div className={`w-10 h-6 rounded-full p-1 transition-colors shrink-0 border-[3px] border-black ${checked ? 'bg-black' : 'bg-white'}`}>
        <div className={`w-3 h-3 rounded-full transition-transform ${checked ? 'translate-x-4 bg-white' : 'translate-x-0 bg-black'}`} />
      </div>
      <input type="checkbox" className="hidden" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

export default function Settings() {
  const {
    socksPort, setSocksPort,
    httpPort, setHttpPort,
    networkStack, setNetworkStack,
    dnsMode, setDnsMode,
    strictRoute, setStrictRoute,
    killSwitch, setKillSwitch,
    silentAdminAutostart, setSilentAdminAutostart,
    autoConnectOnStartup, setAutoConnectOnStartup,
    language, setLanguage,
    addLog,
    clearLogs,
    wipeData,
  } = useAppStore();
  const { t } = useTranslation();

  const handleWipeData = () => {
    if (confirm('Are you absolutely sure you want to delete ALL servers and subscriptions? This cannot be undone.')) {
      wipeData();
      addLog('warning', 'All server configurations have been wiped from the device.');
    }
  };

  const handleClearLogs = () => {
    if (confirm('Are you sure you want to clear all connection logs?')) {
      clearLogs();
      addLog('success', 'Runtime logs cleared by user.');
    }
  };

  const [defenderStatus, setDefenderStatus] = useState<string | null>(null);
  const [defenderLoading, setDefenderLoading] = useState(false);

  // Check Defender exclusion status on mount
  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const isExcluded: boolean = await invoke('check_defender_exclusion');
        if (isExcluded) {
          setDefenderStatus('✓ DoodleRay is whitelisted in Windows Defender');
        }
      } catch { /* not in tauri env */ }
    })();
  }, []);

  const handleDefenderExclusion = async () => {
    setDefenderLoading(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result: string = await invoke('add_defender_exclusion');
      setDefenderStatus(result);
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast('Defender exclusion added ✓', 'success');
    } catch (e: any) {
      setDefenderStatus('Failed: ' + (e?.toString() || 'Unknown error'));
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast('Defender exclusion failed (need admin)', 'error');
    } finally {
      setDefenderLoading(false);
    }
  };

  const handleAdminAutostartToggle = async (val: boolean) => {
    // Optimistically update UI
    setSilentAdminAutostart(val);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('toggle_silent_autostart', { enable: val });
      // When enabling silent admin autostart, disable regular autostart to avoid duplicates
      if (val) {
        try {
          await disable();
          useAppStore.setState({ autoStart: false });
        } catch (_) { /* ignore if already disabled */ }
      }
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast(
        val ? 'Admin autostart enabled ✓' : 'Admin autostart disabled',
        'success'
      );
    } catch (e: any) {
      // Revert on failure (e.g. UAC declined)
      setSilentAdminAutostart(!val);
      addLog('error', `Failed to toggle admin autostart: ${e}`);
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast(
        `Autostart failed: ${e?.toString()?.replace('Error: ', '') || 'UAC declined'}`,
        'error'
      );
    }
  };

  const [updateStatus, setUpdateStatus] = useState<string>('');
  const [appVersion, setAppVersion] = useState<string>('...');

  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(setAppVersion).catch(() => {});
  }, []);

  const handleCheckUpdate = async () => {
    setUpdateStatus('Checking...');
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const update = await check();
      if (update) {
        setUpdateStatus(`v${update.version} available! Downloading...`);
        await update.downloadAndInstall();
        setUpdateStatus('Update installed! Restarting...');
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch();
      } else {
        setUpdateStatus('You are on the latest version ✓');
        setTimeout(() => setUpdateStatus(''), 3000);
      }
    } catch (e: any) {
      setUpdateStatus(`Update check failed: ${e.message || e}`);
      setTimeout(() => setUpdateStatus(''), 5000);
    }
  };

  return (
    <div className="flex-1 p-5 md:p-8 overflow-y-auto animate-fade-in">
      <div className="max-w-4xl mx-auto">
        <h1 className="text-3xl font-black text-black flex items-center gap-4 drop-shadow-[2px_2px_0_#fff] mb-10 tracking-tighter uppercase">
          <span className="p-3 bg-black text-white rounded-xl shadow-[4px_4px_0_#000] border-[3px] border-black"><SettingsIcon className="w-6 h-6 stroke-[3px]" /></span>
          {t('preferences')}
        </h1>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          
          {/* Section 1: System */}
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000]">
            <h2 className="text-xl font-black text-black mb-5 flex items-center gap-2 uppercase tracking-tight bg-white px-3 py-1 w-max rounded-lg border-[3px] border-black shadow-[2px_2px_0_#000]">
              <Monitor className="w-5 h-5 text-black stroke-[3px]" /> {t('system')}
            </h2>
            <div className="space-y-2">
              <Toggle
                checked={silentAdminAutostart}
                onChange={handleAdminAutostartToggle}
                label={t('launchStartup')}
                description={t('launchStartupDesc')}
              />
              <Toggle
                checked={autoConnectOnStartup}
                onChange={setAutoConnectOnStartup}
                label={t('autoConnect')}
                description={t('autoConnectDesc')}
              />
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('language')}</span>
                <select value={language} onChange={(e) => setLanguage(e.target.value as any)}
                  className="bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-lg px-3 py-1.5 text-xs text-black font-black uppercase tracking-widest focus:outline-none cursor-pointer">
                  <option value="en">English</option>
                  <option value="ru">Русский</option>
                  <option value="zh">中文</option>
                </select>
              </div>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('socksPort')}</span>
                <input type="number" value={socksPort} onChange={(e) => setSocksPort(parseInt(e.target.value) || 10808)}
                  className="w-24 bg-white border-[3px] border-black shadow-inner rounded-lg px-3 py-1.5 text-sm font-black text-black focus:outline-none text-center" />
              </div>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('httpPort')}</span>
                <input type="number" value={httpPort} onChange={(e) => setHttpPort(parseInt(e.target.value) || 10809)}
                  className="w-24 bg-white border-[3px] border-black shadow-inner rounded-lg px-3 py-1.5 text-sm font-black text-black focus:outline-none text-center" />
              </div>
              <p className="text-[10px] font-black text-text-on-orange-secondary/70 px-2 uppercase tracking-widest mt-1">
                {t('portChangeHint')}
              </p>
            </div>
          </div>

          {/* Section 2: Core Engine */}
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000]">
            <h2 className="text-xl font-black text-black mb-5 flex items-center gap-2 uppercase tracking-tight bg-white px-3 py-1 w-max rounded-lg border-[3px] border-black shadow-[2px_2px_0_#000]">
              <Zap className="w-5 h-5 text-black stroke-[3px]" /> {t('coreEngine')}
            </h2>
            <div className="space-y-2">
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('dns')}</span>
                <select value={dnsMode} onChange={(e) => setDnsMode(e.target.value as any)}
                  className="bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-lg px-3 py-1.5 text-xs text-black font-black uppercase tracking-widest focus:outline-none cursor-pointer">
                  <option value="fakeip">Fake-IP (Fast)</option>
                  <option value="realip">Real-IP</option>
                </select>
              </div>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('l3Stack')}</span>
                <select value={networkStack} onChange={(e) => setNetworkStack(e.target.value as any)}
                  className="bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-lg px-3 py-1.5 text-xs text-black font-black uppercase tracking-widest focus:outline-none cursor-pointer">
                  <option value="mixed">Mixed</option>
                  <option value="system">System</option>
                  <option value="gvisor">gVisor</option>
                </select>
              </div>
              <Toggle
                checked={strictRoute}
                onChange={setStrictRoute}
                label={t('strictRoute')}
                description={t('strictRouteDesc')}
              />
              <Toggle
                checked={killSwitch}
                onChange={setKillSwitch}
                label={t('killSwitch')}
                description={t('killSwitchDesc')}
              />
            </div>
          </div>

          {/* Section 3: Data */}
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000] lg:col-span-2">
            <h2 className="text-xl font-black text-black mb-5 flex items-center gap-2 uppercase tracking-tight bg-white px-3 py-1 w-max rounded-lg border-[3px] border-black shadow-[2px_2px_0_#000]">
              <Database className="w-5 h-5 text-black stroke-[3px]" /> {t('data')}
            </h2>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <button onClick={handleClearLogs} className="group flex items-center gap-4 bg-white border-[3px] border-black shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none p-5 rounded-2xl transition-all cursor-pointer text-left">
                <div className="w-12 h-12 rounded-xl border-[3px] border-black bg-black text-white flex items-center justify-center shrink-0">
                  <RotateCcw className="w-6 h-6 stroke-[3px] transition-transform duration-500 group-hover:-rotate-180" />
                </div>
                <div className="flex-1 min-w-0">
                  <h3 className="font-black text-black text-sm uppercase tracking-tight">{t('clearLogs')}</h3>
                  <p className="text-[10px] font-black tracking-widest uppercase text-black/60 mt-1">{t('clearLogsDesc')}</p>
                </div>
              </button>
              <button onClick={handleWipeData} className="flex items-center gap-4 bg-white border-[3px] border-danger shadow-[4px_4px_0_#f87171] hover:bg-danger hover:border-black hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none p-5 rounded-2xl transition-all cursor-pointer group text-left">
                <div className="w-12 h-12 rounded-xl border-[3px] border-danger group-hover:border-black bg-danger group-hover:bg-black text-white flex items-center justify-center shrink-0 transition-colors">
                  <Trash2 className="w-6 h-6 stroke-[3px] transition-transform duration-300 group-hover:scale-110 group-hover:rotate-12" />
                </div>
                <div className="flex-1 min-w-0">
                  <h3 className="font-black text-danger group-hover:text-black text-sm uppercase tracking-tight transition-colors">{t('factoryReset')}</h3>
                  <p className="text-[10px] font-black tracking-widest uppercase text-danger/80 group-hover:text-black mt-1 transition-colors">{t('factoryResetDesc')}</p>
                </div>
              </button>
            </div>

            {/* Windows Defender exclusion */}
            <button onClick={handleDefenderExclusion} disabled={defenderLoading} className={`group flex items-center gap-4 bg-white border-[3px] border-black shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none p-5 rounded-2xl transition-all cursor-pointer text-left col-span-full ${defenderLoading ? 'opacity-60 cursor-wait' : ''}`}>
              <div className={`w-12 h-12 rounded-xl border-[3px] border-black ${defenderStatus?.startsWith('✓') ? 'bg-emerald-500' : 'bg-emerald-400'} text-black flex items-center justify-center shrink-0`}>
                <ShieldCheck className={`w-6 h-6 stroke-[3px] transition-transform duration-300 group-hover:scale-110 ${defenderLoading ? 'animate-pulse' : ''}`} />
              </div>
              <div className="flex-1 min-w-0">
                <h3 className="font-black text-black text-sm uppercase tracking-tight">Windows Defender Exclusion</h3>
                <p className="text-[10px] font-black tracking-widest uppercase text-black/60 mt-1">
                  {defenderLoading ? 'Applying exclusion...' : 'Add DoodleRay to Defender whitelist — prevents false positives'}
                </p>
                {defenderStatus && <p className={`text-[9px] font-bold mt-1 ${defenderStatus.startsWith('✓') ? 'text-emerald-600' : 'text-red-600'}`}>{defenderStatus}</p>}
              </div>
            </button>
          </div>

        </div>

        {/* Footer */}
        <div className="mt-10 text-center pb-8">
          <button onClick={handleCheckUpdate}
            className="group inline-flex items-center gap-2 px-5 py-2.5 bg-white border-[3px] border-black shadow-[4px_4px_0_#000] hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none rounded-xl transition-all cursor-pointer mb-4">
            <Download className="w-4 h-4 stroke-[3px] transition-transform duration-300 group-hover:-translate-y-1" />
            <span className="text-xs font-black uppercase tracking-widest">{t('checkForUpdates')}</span>
          </button>
          {updateStatus && (
            <p className="text-[11px] font-black text-black/80 mb-3 uppercase tracking-widest">{updateStatus}</p>
          )}
          <p className="text-sm font-black text-text-on-orange-secondary/40 tracking-widest mt-3">DoodleRay v{appVersion}</p>
        </div>
        
      </div>
    </div>
  );
}
