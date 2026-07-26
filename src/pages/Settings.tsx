import { useState, useEffect } from 'react';
import { Settings as SettingsIcon, Trash2, RotateCcw, Database, Zap, Monitor, Download, ShieldCheck, ChevronDown, RefreshCw, Network, HardDrive, ClipboardCopy, Loader2, Wrench } from 'lucide-react';
import { disable } from '@tauri-apps/plugin-autostart';
import { useTranslation } from '../locales';
import { useAppStore } from '../stores/app-store';
import { checkForAppUpdate, getCachedUpdate, installAppUpdate } from '../lib/app-updater';
import { clearAppCache, diagnosticsReportToText, getStorageReport, runNetworkDiagnostics, type DiagnosticCheck, type NetworkDiagnosticsReport, type StorageReport } from '../lib/diagnostics';
import { reportConnectionError } from '../lib/workshop-api';
import { isDesktopAutostartAvailable } from '../lib/build-policy';
import { desktopBridge } from '../platform/tauri/desktop-bridge.ts';

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

function formatMessage(template: string, values: Record<string, string | number>) {
  return Object.entries(values).reduce(
    (message, [key, value]) => message.replace(new RegExp(`\\{${key}\\}`, 'g'), String(value)),
    template
  );
}

function updatePhaseFromStatus(status: string): 'installing' | 'downloading' {
  return status === 'updateClosingProcesses' ||
    status === 'updatePreparingInstall' ||
    status === 'updateInstallingRestarting'
    ? 'installing'
    : 'downloading';
}

function updateStatusLabel(
  status: string,
  phase: string,
  progress: number | null,
  version: string | null,
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
      return version
        ? formatMessage(t('updateDownloadingVersion'), { version })
        : t('updateDownloading');
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

const diagnosticTitles: Record<string, Record<string, string>> = {
  ru: {
    hosts_override: 'Подписка найдена в hosts',
    subscription_dns_public: 'DNS подписки ведет на публичные IP',
    subscription_dns_private: 'DNS подписки ведет на приватный/локальный IP',
    subscription_dns_failed: 'DNS подписки не резолвится',
    subscription_not_checked: 'Подписка не проверялась',
    subscription_fetch_blocked: 'Загрузка подписки заблокирована до HTTP-запроса',
    subscription_fetch_client_failed: 'HTTP-клиент подписки не запустился',
    subscription_http_status_bad: 'Подписка вернула HTTP-ошибку',
    subscription_body_empty: 'Подписка вернула пустой ответ',
    subscription_fetch_ok: 'Подписка загрузилась',
    subscription_body_read_failed: 'Не удалось прочитать тело подписки',
    subscription_fetch_failed: 'HTTP-запрос подписки не прошел',
    conflicts_none_detected: 'Известных конфликтующих программ не найдено',
    conflicts_detected: 'Найдены возможные конфликтующие программы',
    network_services_detected: 'Найдены возможные конфликтующие сетевые службы',
    network_services_clean: 'Конфликтующие службы Windows не найдены',
    socks_port_busy: 'SOCKS порт',
    socks_port_free: 'SOCKS порт',
    http_port_busy: 'HTTP порт',
    http_port_free: 'HTTP порт',
    public_tcp_443: 'Проверка публичного TCP/443',
    system_dns_resolve: 'Проверка системного DNS',
    active_server_tcp: 'Доступность активного сервера по TCP',
    active_server_udp_protocol: 'Активный сервер использует UDP-протокол',
    active_server_dns_ok: 'DNS активного сервера резолвится',
    active_server_dns_failed: 'DNS активного сервера не резолвится',
    active_server_not_selected: 'Активный сервер не выбран',
    socks_handshake_failed: 'SOCKS порт отвечает некорректно',
    socks_handshake_ok: 'SOCKS5 handshake прошел',
    socks_handshake_rejected: 'SOCKS5 handshake отклонен',
    socks_handshake_timeout: 'SOCKS порт не ответил на handshake',
    socks_port_closed: 'SOCKS порт закрыт',
    split_rules_proxy_mode: 'Правила Мастерской работают в режиме «Весь компьютер»',
    split_rules_tun_mode: 'Правила Мастерской могут примениться',
    default_route_unavailable: 'Default route недоступен',
    default_route_snapshot: 'Снимок default route',
    dns_snapshot_unavailable: 'Снимок DNS недоступен',
    dns_snapshot: 'Снимок DNS-резолверов',
    app_network_settings: 'Сетевые настройки DoodleRay',
  },
  zh: {
    hosts_override: '订阅主机出现在 hosts 文件中',
    subscription_dns_public: '订阅 DNS 解析到公网 IP',
    subscription_dns_private: '订阅 DNS 解析到私有/本地 IP',
    subscription_dns_failed: '订阅 DNS 解析失败',
    subscription_not_checked: '未检查订阅',
    subscription_fetch_blocked: 'HTTP 前订阅拉取被阻止',
    subscription_fetch_client_failed: '订阅 HTTP 客户端初始化失败',
    subscription_http_status_bad: '订阅返回 HTTP 错误',
    subscription_body_empty: '订阅返回空内容',
    subscription_fetch_ok: '订阅拉取成功',
    subscription_body_read_failed: '订阅内容读取失败',
    subscription_fetch_failed: '订阅 HTTP 请求失败',
    conflicts_none_detected: '未检测到已知冲突程序',
    conflicts_detected: '检测到可能冲突的程序',
    network_services_detected: '检测到可能冲突的网络服务',
    network_services_clean: '未检测到冲突的 Windows 服务',
    socks_port_busy: 'SOCKS 端口',
    socks_port_free: 'SOCKS 端口',
    http_port_busy: 'HTTP 端口',
    http_port_free: 'HTTP 端口',
    public_tcp_443: '公网 TCP/443 检查',
    system_dns_resolve: '系统 DNS 检查',
    active_server_tcp: '活动服务器 TCP 可达性',
    active_server_udp_protocol: '活动服务器使用 UDP 协议',
    active_server_dns_ok: '活动服务器 DNS 已解析',
    active_server_dns_failed: '活动服务器 DNS 解析失败',
    active_server_not_selected: '未选择活动服务器',
    socks_handshake_failed: 'SOCKS 端口响应异常',
    socks_handshake_ok: 'SOCKS5 握手通过',
    socks_handshake_rejected: 'SOCKS5 握手被拒绝',
    socks_handshake_timeout: 'SOCKS 端口握手超时',
    socks_port_closed: 'SOCKS 端口已关闭',
    split_rules_proxy_mode: '创意工坊规则需要全设备模式',
    split_rules_tun_mode: '创意工坊规则可应用',
    default_route_unavailable: '默认路由不可用',
    default_route_snapshot: '默认路由快照',
    dns_snapshot_unavailable: 'DNS 快照不可用',
    dns_snapshot: 'DNS 解析器快照',
    app_network_settings: 'DoodleRay 网络设置',
  },
};

const diagnosticSeverityLabels: Record<string, Record<string, string>> = {
  ru: { ok: 'ОК', info: 'ИНФО', warning: 'ВНИМАНИЕ', error: 'ОШИБКА' },
  zh: { ok: '正常', info: '信息', warning: '警告', error: '错误' },
};

function diagnosticTitle(check: DiagnosticCheck, language: string) {
  return diagnosticTitles[language]?.[check.code] || check.title;
}

function diagnosticSeverity(severity: DiagnosticCheck['severity'], language: string) {
  return diagnosticSeverityLabels[language]?.[severity] || severity;
}

export default function Settings() {
  const desktopAutostartAvailable = isDesktopAutostartAvailable();
  const {
    socksPort, setSocksPort,
    httpPort, setHttpPort,
    networkStack, setNetworkStack,
    dnsMode, setDnsMode,
    strictRoute, setStrictRoute,
    killSwitch, setKillSwitch,
    silentAdminAutostart, setSilentAdminAutostart,
    autoConnectOnStartup, setAutoConnectOnStartup,
    subAutoUpdateMinutes, setSubAutoUpdateMinutes,
    showStats, setShowStats,
    language, setLanguage,
    subscriptions,
    availableUpdate,
    updatePhase,
    updateStatus,
    updateProgress,
    setUpdateState,
    addLog,
    clearLogs,
    wipeData,
  } = useAppStore();
  const { t } = useTranslation();

  const [confirmModal, setConfirmModal] = useState<{
    show: boolean;
    title: string;
    message: string;
    onConfirm: () => void;
  }>({ show: false, title: '', message: '', onConfirm: () => {} });

  const handleWipeData = () => {
    setConfirmModal({
      show: true,
      title: t('factoryReset'),
      message: 'Are you absolutely sure you want to delete ALL servers and subscriptions? This cannot be undone.',
      onConfirm: () => {
        wipeData();
        addLog('info', 'All server configurations have been wiped from the device.');
        setConfirmModal(prev => ({ ...prev, show: false }));
      }
    });
  };

  const handleClearLogs = () => {
    setConfirmModal({
      show: true,
      title: t('clearLogs'),
      message: 'Are you sure you want to clear all connection logs?',
      onConfirm: () => {
        clearLogs();
        addLog('success', 'Runtime logs cleared by user.');
        setConfirmModal(prev => ({ ...prev, show: false }));
      }
    });
  };

  const [defenderStatus, setDefenderStatus] = useState<string | null>(null);
  const [defenderLoading, setDefenderLoading] = useState(false);

  // Check Defender exclusion status on mount
  useEffect(() => {
    (async () => {
      try {
        const isExcluded = await desktopBridge.checkDefenderExclusion();
        if (isExcluded) {
          setDefenderStatus('✓ DoodleRay is whitelisted in Windows Defender');
        }
      } catch { /* not in tauri env */ }
    })();
  }, []);

  const handleDefenderExclusion = async () => {
    setDefenderLoading(true);
    try {
      const result = await desktopBridge.addDefenderExclusion();
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
      await desktopBridge.toggleSilentAutostart(val);
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
      
      // If enabling and not already admin, offer to restart as admin right now
      if (val) {
        try {
          const isAdmin = await desktopBridge.isAdmin();
          if (!isAdmin) {
            setConfirmModal({
              show: true,
              title: 'Restart Required',
              message: 'Admin autostart is set for next login.\n\nRestart as Administrator now?\nThis will give full access to Whole computer mode and other admin features immediately.',
              onConfirm: async () => {
                addLog('info', 'Restarting as administrator...');
                await desktopBridge.restartAsAdmin();
                setConfirmModal(prev => ({ ...prev, show: false }));
              }
            });
          }
        } catch (_) { /* ignore */ }
      }
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

  const [appVersion, setAppVersion] = useState<string>('...');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [networkReport, setNetworkReport] = useState<NetworkDiagnosticsReport | null>(null);
  const [storageLoading, setStorageLoading] = useState(false);
  const [storageReport, setStorageReport] = useState<StorageReport | null>(null);
  const [cacheStatus, setCacheStatus] = useState('');
  const [proxyStaleState, setProxyStaleState] = useState<string>('unknown');
  const [proxyRepairLoading, setProxyRepairLoading] = useState(false);
  const [proxyRepairStatus, setProxyRepairStatus] = useState('');
  const updateStatusText = updateStatusLabel(updateStatus, updatePhase, updateProgress, availableUpdate || appVersion, t);
  const showProxyRepair = ['orphaned_managed', 'legacy_disabled_values', 'legacy_enabled_values'].includes(proxyStaleState);

  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(setAppVersion).catch(() => {});
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const state = await invoke<string>('detect_stale_doodleray_proxy');
        setProxyStaleState(state);
      } catch {
        setProxyStaleState('unsupported');
      }
    })();
  }, []);

  const handleRepairStaleProxy = async () => {
    setProxyRepairLoading(true);
    setProxyRepairStatus('');
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const outcome = await invoke<string>('repair_stale_doodleray_proxy_only');
      setProxyRepairStatus(`${t('windowsProxyRepairDone')}: ${outcome}`);
      setProxyStaleState('none');
      addLog('success', `${t('windowsProxyRepair')}: ${outcome}`);
      const { useToastStore } = await import('../stores/toast-store');
      useToastStore.getState().addToast(t('windowsProxyRepairDone'), 'success');
    } catch (e: any) {
      const message = e?.message || String(e);
      setProxyRepairStatus(message);
      addLog('error', `${t('windowsProxyRepairFailed')}: ${message}`);
    } finally {
      setProxyRepairLoading(false);
    }
  };

  const handleCheckUpdate = async () => {
    setUpdateState({
      updatePhase: 'checking',
      updateStatus: 'updateChecking',
      updateProgress: null,
    });
    try {
      const { isUpdateManagedByStore, openStoreUpdatePage } = await import('../lib/update-channel');
      if (isUpdateManagedByStore()) {
        await openStoreUpdatePage();
        setUpdateState({
          availableUpdate: null,
          updatePhase: 'idle',
          updateStatus: 'updateOpenStore',
          updateProgress: null,
        });
        return;
      }

      let update = getCachedUpdate();
      if (!update) {
        update = await checkForAppUpdate();
      }
      if (update) {
        // store-win32 policy: show availability, open Store/support page,
        // never silently download in-app when self-update is disabled.
        const { isInAppUpdateEnabled, openStoreUpdatePage } = await import('../lib/update-channel');
        if (!isInAppUpdateEnabled()) {
          await openStoreUpdatePage();
          setUpdateState({
            availableUpdate: update.version,
            updatePhase: 'available',
            updateStatus: 'updateOpenStore',
            updateProgress: null,
          });
          return;
        }
        setUpdateState({
          availableUpdate: update.version,
          updatePhase: 'downloading',
          updateStatus: 'updateDownloading',
          updateProgress: 0,
        });
        await installAppUpdate({
          update,
          onStatus: (status) => {
            setUpdateState({
              updateStatus: status,
              updatePhase: updatePhaseFromStatus(status),
            });
          },
          onProgress: (progress) => setUpdateState({ updateProgress: progress }),
        });
      } else {
        setUpdateState({
          availableUpdate: null,
          updatePhase: 'idle',
          updateStatus: 'updateLatest',
          updateProgress: null,
        });
        setTimeout(() => setUpdateState({ updateStatus: '' }), 3000);
      }
    } catch (e: any) {
      setUpdateState({
        updatePhase: 'error',
        updateStatus: `${t('updateCheckFailed')}: ${e.message || e}`,
        updateProgress: null,
      });
      setTimeout(() => setUpdateState({ updateStatus: '' }), 5000);
    }
  };

  const handleRunDiagnostics = async () => {
    setDiagnosticsLoading(true);
    try {
      const report = await runNetworkDiagnostics(subscriptions[0]?.url);
      setNetworkReport(report);
      const issueCount = report.checks.filter((check) => check.severity === 'error' || check.severity === 'warning').length;
      const duration = typeof report.durationMs === 'number' ? ` (${(report.durationMs / 1000).toFixed(1)}s)` : '';
      addLog(issueCount > 0 ? 'warning' : 'success', `${formatMessage(t('networkDiagnosticsComplete'), { count: issueCount })}${duration}`);
      for (const check of report.checks.filter((item) => item.severity === 'error' || item.severity === 'warning').slice(0, 5)) {
        addLog(check.severity === 'error' ? 'error' : 'warning', `${check.title}: ${check.detail}`);
      }
      const hasPrivateDns = report.checks.some((check) => check.code === 'subscription_dns_private');
      if (hasPrivateDns) {
        reportConnectionError({
          eventType: 'dns_private_ip',
          errorMessage: diagnosticsReportToText(report),
          details: { source: 'manual_diagnostics' },
        });
      }
    } catch (e: any) {
      const message = e?.message || String(e);
      addLog('error', `${t('diagnosticsFailed')}: ${message}`);
      reportConnectionError({ eventType: 'app_error', errorMessage: message, details: { source: 'network_diagnostics' } });
    } finally {
      setDiagnosticsLoading(false);
    }
  };

  const handleCopyDiagnostics = async () => {
    if (!networkReport) return;
    await navigator.clipboard.writeText(diagnosticsReportToText(networkReport));
    addLog('success', t('diagnosticsReportCopied'));
  };

  const handleStorageReport = async () => {
    setStorageLoading(true);
    setCacheStatus('');
    try {
      const report = await getStorageReport();
      setStorageReport(report);
      addLog('info', `${t('storageReport')}: ${report.totalSize}`);
      if (report.totalBytes > 5 * 1024 * 1024 * 1024) {
        reportConnectionError({
          eventType: 'cache_too_large',
          errorMessage: `${t('appStorageIs')} ${report.totalSize}`,
          details: {
            total_bytes: report.totalBytes,
            paths: report.paths.map((path) => ({ label: path.label, kind: path.kind, bytes: path.bytes, clearable: path.clearable })),
          },
        });
      }
    } catch (e: any) {
      const message = e?.message || String(e);
      addLog('error', `${t('storageReportFailed')}: ${message}`);
    } finally {
      setStorageLoading(false);
    }
  };

  const handleClearCache = async () => {
    setStorageLoading(true);
    setCacheStatus('');
    try {
      const result = await clearAppCache();
      const removedBytes = result.removed.reduce((sum, item) => sum + item.bytes, 0);
      setCacheStatus(formatMessage(t('cacheClearedFolders'), { count: result.removed.length }));
      addLog('success', `${t('cacheCleared')}: ${(removedBytes / 1024 / 1024).toFixed(1)} MB`);
      const report = await getStorageReport();
      setStorageReport(report);
    } catch (e: any) {
      const message = e?.message || String(e);
      setCacheStatus(`${t('cacheClearFailed')}: ${message}`);
      addLog('error', `${t('cacheClearFailed')}: ${message}`);
      reportConnectionError({ eventType: 'app_error', errorMessage: message, details: { source: 'clear_cache' } });
    } finally {
      setStorageLoading(false);
    }
  };

  return (
    <div className="flex-1 p-5 md:p-8 overflow-y-auto animate-fade-in">
      <div className="mx-auto max-w-5xl">
        <h1 className="text-3xl font-black text-black flex items-center gap-4 drop-shadow-[2px_2px_0_#fff] mb-10 tracking-tighter uppercase">
          <span className="p-3 bg-black text-white rounded-xl shadow-[4px_4px_0_#000] border-[3px] border-black"><SettingsIcon className="w-6 h-6 stroke-[3px]" /></span>
          {t('preferences')}
        </h1>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          
          {/* Section 1: Basic */}
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-5 md:p-6 shadow-[6px_6px_0_#000] lg:col-span-2">
            <h2 className="mb-5 flex w-max max-w-full items-center gap-2 rounded-lg border-[3px] border-black bg-white px-3 py-1 text-lg font-black uppercase tracking-tight text-black shadow-[2px_2px_0_#000] md:text-xl">
              <Monitor className="w-5 h-5 text-black stroke-[3px]" /> {t('basicSettings')}
            </h2>
            <div className="space-y-2">
              {desktopAutostartAvailable && (
                <Toggle
                  checked={silentAdminAutostart}
                  onChange={handleAdminAutostartToggle}
                  label={t('launchStartup')}
                  description={t('launchStartupDesc')}
                />
              )}
              <Toggle
                checked={autoConnectOnStartup}
                onChange={setAutoConnectOnStartup}
                label={t('autoConnect')}
                description={t('autoConnectDesc')}
              />
              <Toggle
                checked={showStats}
                onChange={setShowStats}
                label={t('showLiveStats')}
                description={t('showLiveStatsDesc')}
              />
              <div className="flex items-center justify-between gap-3 py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <div className="flex min-w-0 items-start gap-3">
                  <RefreshCw className="mt-0.5 h-4 w-4 shrink-0 text-black stroke-[3px]" />
                  <div className="min-w-0">
                    <span className="text-sm font-black text-black block uppercase tracking-tight">{t('subAutoUpdate')}</span>
                    <span className="text-[10px] font-black text-black/60 block mt-0.5 tracking-widest uppercase">{t('subAutoUpdateDesc')}</span>
                  </div>
                </div>
                <select value={subAutoUpdateMinutes} onChange={(e) => setSubAutoUpdateMinutes(parseInt(e.target.value, 10))}
                  className="shrink-0 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-lg px-3 py-1.5 text-xs text-black font-black uppercase tracking-widest focus:outline-none cursor-pointer">
                  <option value={0}>{t('disabled')}</option>
                  <option value={60}>{t('everyHour')}</option>
                  <option value={180}>{t('every3Hours')}</option>
                  <option value={360}>{t('every6Hours')}</option>
                  <option value={720}>{t('every12Hours')}</option>
                </select>
              </div>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('language')}</span>
                <select value={language} onChange={(e) => setLanguage(e.target.value as any)}
                  className="bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-lg px-3 py-1.5 text-xs text-black font-black uppercase tracking-widest focus:outline-none cursor-pointer">
                  <option value="en">English</option>
                  <option value="ru">Русский</option>
                  <option value="zh">中文</option>
                </select>
              </div>
              <button
                type="button"
                onClick={() => setShowAdvanced((open) => !open)}
                className="flex w-full items-center justify-between gap-3 rounded-xl border-[3px] border-black bg-black px-4 py-3 text-left text-white shadow-[2px_2px_0_rgba(0,0,0,0.35)] transition-all hover:-translate-y-0.5 hover:shadow-[4px_4px_0_rgba(0,0,0,0.35)] active:translate-y-0 active:shadow-none"
              >
                <span className="min-w-0">
                  <span className="block text-sm font-black uppercase tracking-tight">{t('advancedSettings')}</span>
                  <span className="mt-0.5 block text-[9px] font-black uppercase tracking-widest text-white/55">{t('advancedSettingsDesc')}</span>
                </span>
                <span className="flex shrink-0 items-center gap-1.5 text-[9px] font-black uppercase tracking-widest text-bg-primary">
                  {showAdvanced ? t('hideAdvanced') : t('showAdvanced')}
                  <ChevronDown className={`h-4 w-4 stroke-[3px] transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
                </span>
              </button>
              {showAdvanced && (
                <>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('socksPort')}</span>
                <input type="number" min={1024} max={65535} value={socksPort} onChange={(e) => {
                  const value = Number(e.target.value);
                  setSocksPort(Number.isInteger(value) ? Math.min(65535, Math.max(1024, value)) : 10808);
                }}
                  className="w-24 bg-white border-[3px] border-black shadow-inner rounded-lg px-3 py-1.5 text-sm font-black text-black focus:outline-none text-center" />
              </div>
              <div className="flex items-center justify-between py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl">
                <span className="text-sm font-black text-black uppercase tracking-tight">{t('httpPort')}</span>
                <input type="number" min={1024} max={65535} value={httpPort} onChange={(e) => {
                  const value = Number(e.target.value);
                  setHttpPort(Number.isInteger(value) ? Math.min(65535, Math.max(1024, value)) : 10809);
                }}
                  className="w-24 bg-white border-[3px] border-black shadow-inner rounded-lg px-3 py-1.5 text-sm font-black text-black focus:outline-none text-center" />
              </div>
              <div className="py-3 px-4 bg-white border-[3px] border-black shadow-[2px_2px_0_#000] rounded-xl space-y-2">
                <div className="flex items-center justify-between gap-3">
                  <span className="text-sm font-black text-black uppercase tracking-tight">{t('manualProxyPorts')}</span>
                  <span className="rounded-lg border-[2px] border-black bg-bg-primary px-2 py-1 text-[9px] font-black uppercase tracking-widest text-black">
                    {t('manualProxyModeTitle')}
                  </span>
                </div>
                <div className="grid gap-1.5 text-[10px] font-black uppercase tracking-widest text-black/65">
                  <span>HTTP 127.0.0.1:{httpPort}</span>
                  <span>SOCKS5 127.0.0.1:{socksPort}</span>
                </div>
              </div>
              <p className="text-[10px] font-black text-text-on-orange-secondary/70 px-2 uppercase tracking-widest mt-1">
                {t('portChangeHint')}
              </p>
                </>
              )}
            </div>
          </div>

          {/* Section 2: Core Engine */}
          {showAdvanced && (
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000] lg:col-span-2">
            <h2 className="mb-5 flex w-max max-w-full items-center gap-2 rounded-lg border-[3px] border-black bg-white px-3 py-1 text-lg font-black uppercase tracking-tight text-black shadow-[2px_2px_0_#000] md:text-xl">
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
          )}

          {/* Section 3: Maintenance */}
          <div className="bg-bg-primary border-[4px] border-black rounded-2xl p-6 shadow-[6px_6px_0_#000] lg:col-span-2">
            <h2 className="mb-5 flex w-max max-w-full items-center gap-2 rounded-lg border-[3px] border-black bg-white px-3 py-1 text-lg font-black uppercase tracking-tight text-black shadow-[2px_2px_0_#000] md:text-xl">
              <Database className="w-5 h-5 text-black stroke-[3px]" /> {t('maintenance')}
            </h2>
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
              <button onClick={handleClearLogs} className="group flex min-h-28 cursor-pointer items-center gap-4 rounded-2xl border-[3px] border-black bg-white p-5 text-left shadow-[4px_4px_0_#000] transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none lg:col-span-6">
                <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-xl border-[3px] border-black bg-black text-white">
                  <RotateCcw className="w-6 h-6 stroke-[3px] transition-transform duration-500 group-hover:-rotate-180" />
                </div>
                <div className="flex-1 min-w-0">
                  <h3 className="text-base font-black uppercase tracking-tight text-black">{t('clearLogs')}</h3>
                  <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-black/55">{t('clearLogsDesc')}</p>
                </div>
              </button>
              <button onClick={handleWipeData} className="group flex min-h-28 cursor-pointer items-center gap-4 rounded-2xl border-[3px] border-danger bg-white p-5 text-left shadow-[4px_4px_0_#f87171] transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:border-black hover:bg-danger hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none lg:col-span-6">
                <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-xl border-[3px] border-danger bg-danger text-white transition-colors group-hover:border-black group-hover:bg-black">
                  <Trash2 className="w-6 h-6 stroke-[3px] transition-transform duration-300 group-hover:scale-110 group-hover:rotate-12" />
                </div>
                <div className="flex-1 min-w-0">
                  <h3 className="text-base font-black uppercase tracking-tight text-danger transition-colors group-hover:text-black">{t('factoryReset')}</h3>
                  <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-danger/80 transition-colors group-hover:text-black">{t('factoryResetDesc')}</p>
                </div>
              </button>

            {/* Windows Defender exclusion */}
            <button onClick={handleDefenderExclusion} disabled={defenderLoading} className={`group flex min-h-28 w-full cursor-pointer items-center gap-4 rounded-2xl border-[3px] border-black bg-white p-5 text-left shadow-[4px_4px_0_#000] transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none lg:col-span-12 ${defenderLoading ? 'cursor-wait opacity-60' : ''}`}>
              <div className={`w-12 h-12 rounded-xl border-[3px] border-black ${defenderStatus?.startsWith('✓') ? 'bg-emerald-500' : 'bg-emerald-400'} text-black flex items-center justify-center shrink-0`}>
                <ShieldCheck className={`w-6 h-6 stroke-[3px] transition-transform duration-300 group-hover:scale-110 ${defenderLoading ? 'animate-pulse' : ''}`} />
              </div>
              <div className="flex-1 min-w-0">
                <h3 className="text-base font-black uppercase tracking-tight text-black">{t('defenderExclusion')}</h3>
                <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-black/55">
                  {defenderLoading ? t('applyingExclusion') : t('defenderExclusionDesc')}
                </p>
                {defenderStatus && <p className={`text-[9px] font-bold mt-1 ${defenderStatus.startsWith('✓') ? 'text-emerald-600' : 'text-red-600'}`}>{defenderStatus}</p>}
              </div>
            </button>

            {showProxyRepair && (
              <button onClick={handleRepairStaleProxy} disabled={proxyRepairLoading} className={`group flex min-h-28 w-full cursor-pointer items-center gap-4 rounded-2xl border-[3px] border-black bg-white p-5 text-left shadow-[4px_4px_0_#000] transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none lg:col-span-12 ${proxyRepairLoading ? 'cursor-wait opacity-60' : ''}`}>
                <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border-[3px] border-black bg-amber-300 text-black">
                  {proxyRepairLoading ? <Loader2 className="h-6 w-6 animate-spin stroke-[3px]" /> : <Wrench className="h-6 w-6 stroke-[3px] transition-transform duration-300 group-hover:-rotate-12" />}
                </div>
                <div className="min-w-0 flex-1">
                  <h3 className="text-base font-black uppercase tracking-tight text-black">{t('windowsProxyRepair')}</h3>
                  <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-black/55">
                    {proxyRepairLoading ? t('windowsProxyRepairing') : t('windowsProxyRepairDesc')}
                  </p>
                  {proxyRepairStatus && <p className="mt-1 text-[9px] font-bold text-black/60">{proxyRepairStatus}</p>}
                </div>
              </button>
            )}

            <div className="contents">
              <div className="flex min-h-80 flex-col rounded-2xl border-[3px] border-black bg-white p-4 shadow-[4px_4px_0_#000] lg:col-span-6">
                <div className="mb-4 flex items-start gap-3">
                  <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border-[3px] border-black bg-amber-300 text-black">
                    <Network className="h-5 w-5 stroke-[3px]" />
                  </div>
                  <div className="min-w-0">
                    <h3 className="text-base font-black uppercase tracking-tight text-black">{t('networkDiagnostics')}</h3>
                    <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-black/55">{t('networkDiagnosticsDesc')}</p>
                  </div>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    onClick={handleRunDiagnostics}
                    disabled={diagnosticsLoading}
                    className="inline-flex min-h-10 items-center gap-2 rounded-xl border-[2px] border-black bg-black px-4 py-2 text-[10px] font-black uppercase tracking-widest text-white shadow-[2px_2px_0_#000] transition-all hover:-translate-y-0.5 active:translate-y-1 active:shadow-none disabled:opacity-50"
                  >
                    <RefreshCw className={`h-3.5 w-3.5 stroke-[3px] ${diagnosticsLoading ? 'animate-spin' : ''}`} />
                    {t('runDiagnostics')}
                  </button>
                  <button
                    onClick={handleCopyDiagnostics}
                    disabled={!networkReport}
                    className="inline-flex min-h-10 items-center gap-2 rounded-xl border-[2px] border-black bg-white px-4 py-2 text-[10px] font-black uppercase tracking-widest text-black shadow-[2px_2px_0_#000] transition-all hover:-translate-y-0.5 active:translate-y-1 active:shadow-none disabled:opacity-40"
                  >
                    <ClipboardCopy className="h-3.5 w-3.5 stroke-[3px]" />
                    {t('copyReport')}
                  </button>
                </div>
                <div className="mt-4 min-h-44 flex-1 overflow-hidden rounded-xl border-[2px] border-black/20 bg-bg-primary/85 p-2">
                {networkReport ? (
                  <div className="max-h-52 space-y-2 overflow-y-auto pr-1">
                    {networkReport.checks.map((check) => (
                      <div key={check.code} className="rounded-lg bg-white/85 px-2.5 py-2">
                        <p className={`text-[9px] font-black uppercase tracking-widest ${
                          check.severity === 'error' ? 'text-red-600' :
                          check.severity === 'warning' ? 'text-amber-700' :
                          check.severity === 'ok' ? 'text-emerald-700' :
                          'text-black/55'
                        }`}>{diagnosticSeverity(check.severity, language)} | {diagnosticTitle(check, language)}</p>
                        <p className="mt-0.5 text-[9px] font-bold leading-relaxed text-black/60">{check.detail}</p>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="h-full min-h-40 rounded-lg border-[2px] border-dashed border-black/20 bg-white/25" />
                )}
                </div>
              </div>

              <div className="flex min-h-80 flex-col rounded-2xl border-[3px] border-black bg-white p-4 shadow-[4px_4px_0_#000] lg:col-span-6">
                <div className="mb-4 flex items-start gap-3">
                  <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border-[3px] border-black bg-sky-300 text-black">
                    <HardDrive className="h-5 w-5 stroke-[3px]" />
                  </div>
                  <div className="min-w-0">
                    <h3 className="text-base font-black uppercase tracking-tight text-black">{t('storageTitle')}</h3>
                    <p className="mt-1 text-[11px] font-black uppercase leading-relaxed tracking-widest text-black/55">{t('storageDesc')}</p>
                  </div>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    onClick={handleStorageReport}
                    disabled={storageLoading}
                    className="inline-flex min-h-10 items-center gap-2 rounded-xl border-[2px] border-black bg-black px-4 py-2 text-[10px] font-black uppercase tracking-widest text-white shadow-[2px_2px_0_#000] transition-all hover:-translate-y-0.5 active:translate-y-1 active:shadow-none disabled:opacity-50"
                  >
                    <RefreshCw className={`h-3.5 w-3.5 stroke-[3px] ${storageLoading ? 'animate-spin' : ''}`} />
                    {t('scanStorage')}
                  </button>
                  <button
                    onClick={handleClearCache}
                    disabled={storageLoading}
                    className="inline-flex min-h-10 items-center gap-2 rounded-xl border-[2px] border-black bg-amber-300 px-4 py-2 text-[10px] font-black uppercase tracking-widest text-black shadow-[2px_2px_0_#000] transition-all hover:-translate-y-0.5 active:translate-y-1 active:shadow-none disabled:opacity-50"
                  >
                    <Trash2 className="h-3.5 w-3.5 stroke-[3px]" />
                    {t('clearCache')}
                  </button>
                </div>
                {cacheStatus && <p className="mt-2 text-[10px] font-black uppercase tracking-widest text-black/65">{cacheStatus}</p>}
                <div className="mt-4 min-h-44 flex-1 overflow-hidden rounded-xl border-[2px] border-black/20 bg-bg-primary/85 p-2">
                {storageReport ? (
                  <div className="max-h-52 space-y-2 overflow-y-auto pr-1">
                    <p className="px-1 text-[10px] font-black uppercase tracking-widest text-black">{t('storageTotal')}: {storageReport.totalSize}</p>
                    {storageReport.paths.map((path) => (
                      <div key={path.path} className="rounded-lg bg-white/85 px-2.5 py-2">
                        <div className="flex items-center justify-between gap-2">
                          <p className="truncate text-[9px] font-black uppercase tracking-widest text-black">{path.label}</p>
                          <span className={`shrink-0 rounded-md border-[1px] border-black px-1.5 py-0.5 text-[8px] font-black uppercase tracking-widest ${
                            path.clearable ? 'bg-amber-200 text-black' : 'bg-black text-white'
                          }`}>{path.size}</span>
                        </div>
                        <p className="mt-0.5 truncate text-[8px] font-bold text-black/45">{path.path}</p>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="h-full min-h-40 rounded-lg border-[2px] border-dashed border-black/20 bg-white/25" />
                )}
                </div>
              </div>
            </div>
            </div>
          </div>

        </div>

        {/* Footer */}
        <div className="mt-10 text-center pb-8">
          <button
            onClick={handleCheckUpdate}
            disabled={updatePhase === 'checking' || updatePhase === 'downloading' || updatePhase === 'installing'}
            className="group mb-4 inline-flex cursor-pointer items-center gap-2 rounded-xl border-[3px] border-black bg-white px-5 py-2.5 shadow-[4px_4px_0_#000] transition-all hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0_#000] active:translate-x-[2px] active:translate-y-[2px] active:shadow-none disabled:cursor-wait disabled:opacity-70"
          >
            {updatePhase === 'checking' || updatePhase === 'downloading' || updatePhase === 'installing' ? (
              <Loader2 className="h-4 w-4 animate-spin stroke-[3px]" />
            ) : (
              <Download className="h-4 w-4 stroke-[3px] transition-transform duration-300 group-hover:-translate-y-1" />
            )}
            <span className="text-xs font-black uppercase tracking-widest">
              {updatePhase === 'downloading' && updateProgress !== null ? `${updateProgress}%` : t('checkForUpdates')}
            </span>
          </button>
          {updateStatusText && (
            <div className="mx-auto mb-3 max-w-xs">
              <p className="text-[11px] font-black uppercase tracking-widest text-black/80">{updateStatusText}</p>
              {updatePhase === 'downloading' && updateProgress !== null && (
                <div className="mt-2 h-2 overflow-hidden rounded-full border-[2px] border-black bg-black/10">
                  <div className="h-full bg-black transition-all duration-300" style={{ width: `${updateProgress}%` }} />
                </div>
              )}
            </div>
          )}
          <p className="text-sm font-black text-text-on-orange-secondary/40 tracking-widest mt-3">DoodleRay v{appVersion}</p>
        </div>
        
      </div>

      {/* ── CUSTOM CONFIRM MODAL ── */}
      {confirmModal.show && (
        <>
          <div className="fixed inset-0 z-[60] bg-black/40 backdrop-blur-sm"
            onClick={() => setConfirmModal(prev => ({ ...prev, show: false }))} />
          <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[70] w-72 bg-white border-[3px] border-black rounded-2xl p-5 shadow-[6px_6px_0_#000] animate-slide-up flex flex-col gap-4">
            <div>
              <h3 className="text-xs font-black uppercase tracking-widest leading-tight">{confirmModal.title}</h3>
              <p className="text-xs text-black/60 font-bold mt-2 leading-relaxed whitespace-pre-wrap">{confirmModal.message}</p>
            </div>
            <div className="flex gap-2 mt-2">
              <button 
                onClick={() => setConfirmModal(prev => ({ ...prev, show: false }))}
                className="flex-1 py-2 bg-white text-black border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer hover:bg-black/5 hover:-translate-y-0.5 active:translate-y-0 transition-all">
                {t('cancel')}
              </button>
              <button 
                onClick={confirmModal.onConfirm}
                className="flex-1 py-2 bg-black text-white border-[2px] border-black rounded-xl text-[10px] font-black uppercase tracking-widest cursor-pointer shadow-[2px_2px_0_#000] hover:shadow-[3px_3px_0_#000] hover:-translate-y-0.5 active:translate-y-1 active:shadow-none transition-all">
                OK
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
