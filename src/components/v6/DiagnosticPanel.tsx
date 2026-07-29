import { useCallback, useEffect, useState } from 'react';
import {
  X, Stethoscope, Loader2, Wrench, Copy, Check, FileDown, ChevronDown,
  ShieldCheck, ShieldAlert, ShieldX,
} from 'lucide-react';
import { useAppStore } from '../../stores/app-store';
import { getActiveRoutingRules, resolveSystemProxyModeForRouting } from '../../lib/connect-helpers';
import { desktopBridge } from '../../platform/tauri/desktop-bridge';

type T = (key: never) => string;

interface DiagnosticCheck {
  id: string;
  label: string;
  status: 'ok' | 'info' | 'warning' | 'error';
  user_text: string;
  technical_detail_redacted: string;
  source: string;
}

export interface NetworkDiagnosisReport {
  overall: 'ok' | 'degraded' | 'limited' | 'failed' | 'repairing';
  user_title: string;
  user_summary: string;
  primary_cause_code?: string;
  user_actions: string[];
  support_summary: string;
  checks: DiagnosticCheck[];
  copy_text: string;
  can_auto_repair: boolean;
  bundle_available: boolean;
}

const OVERALL_META: Record<NetworkDiagnosisReport['overall'], { color: string; Icon: typeof ShieldCheck }> = {
  ok: { color: '#3ddc84', Icon: ShieldCheck },
  degraded: { color: '#ffb02e', Icon: ShieldAlert },
  limited: { color: '#ffb02e', Icon: ShieldAlert },
  repairing: { color: '#FF9E38', Icon: Wrench },
  failed: { color: '#ff6b5a', Icon: ShieldX },
};

const CHECK_COLORS: Record<DiagnosticCheck['status'], string> = {
  ok: '#3ddc84',
  info: 'rgba(255,255,255,0.5)',
  warning: '#ffb02e',
  error: '#ff6b5a',
};

/**
 * Localize the diagnosis by cause code; falls back to the backend's Russian
 * text for unknown codes. Locale entries are "Title|Summary".
 */
function localizedCause(t: T, report: NetworkDiagnosisReport): { title: string; summary: string } {
  const code = report.primary_cause_code;
  if (code) {
    const key = `v6DiagC_${code}`;
    const raw = t(key as never);
    if (raw && raw !== key && raw.includes('|')) {
      const [title, summary] = raw.split('|');
      return { title, summary };
    }
  }
  return { title: report.user_title, summary: report.user_summary };
}

function isTauri() {
  return typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== 'undefined';
}

interface Props {
  onClose: () => void;
  onExportSupportBundle: () => void | Promise<void>;
  t: T;
}

/**
 * Support-grade network diagnosis: human summary + hidden technical block.
 * Runs the real run_network_diagnosis command (service-truth health mapped
 * to plain-language causes); repair drives repair_windows_runtime.
 */
export default function DiagnosticPanel({ onClose, onExportSupportBundle, t }: Props) {
  const [report, setReport] = useState<NetworkDiagnosisReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [copied, setCopied] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [techOpen, setTechOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runDiagnosis = useCallback(async (repairAttempted = false) => {
    if (!isTauri()) { setError('dev'); return; }
    setChecking(true);
    setError(null);
    setCopied(false);
    try {
      const s = useAppStore.getState();
      const routingRules = s.proxyMode === 'tun' ? await getActiveRoutingRules() : [];
      const systemProxyMode = resolveSystemProxyModeForRouting(
        s.proxyMode,
        s.systemProxyMode,
        routingRules,
      );
      const result = await desktopBridge.command<NetworkDiagnosisReport>('run_network_diagnosis', {
        proxyMode: s.proxyMode,
        systemProxyMode,
        socksPort: s.socksPort,
        httpPort: s.httpPort,
        lastSubscriptionError: null,
        repairAttempted,
      });
      setReport(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => { runDiagnosis(); }, [runDiagnosis]);

  const handleRepair = async () => {
    if (repairing) return;
    setRepairing(true);
    try {
      const message = await desktopBridge.command<string>('repair_windows_runtime');
      useAppStore.getState().addLog('info', message.split('\n')[0]);
    } catch (err) {
      useAppStore.getState().addLog('error', `Repair failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setRepairing(false);
    }
    await runDiagnosis(true);
  };

  const handleCopy = async () => {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report.copy_text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch { /* clipboard unavailable */ }
  };

  const handleExport = async () => {
    if (exporting) return;
    setExporting(true);
    try { await onExportSupportBundle(); } finally { setExporting(false); }
  };

  const meta = report ? OVERALL_META[report.overall] ?? OVERALL_META.degraded : null;
  const cause = report ? localizedCause(t, report) : null;
  const showWarningsAsProblems = report
    ? !['all_ok', 'ipv6_quic_unverified'].includes(report.primary_cause_code ?? '')
    : false;
  const problems = report?.checks.filter((c) => (
    c.status === 'error' || (showWarningsAsProblems && c.status === 'warning')
  )) ?? [];

  return (
    <div
      onClick={onClose}
      className="v6-fadein absolute inset-0 z-20 flex items-center justify-center"
      style={{ background: 'rgba(10,5,8,0.5)', backdropFilter: 'blur(8px)', WebkitBackdropFilter: 'blur(8px)' }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="v6-modal flex max-h-[calc(100vh-88px)] w-[min(470px,calc(100vw-48px))] flex-col rounded-[28px] p-[26px] pt-[22px]"
      >
        <div className="mb-3 flex shrink-0 items-center justify-between">
          <span className="flex items-center gap-2 text-[18px] font-semibold text-white">
            <Stethoscope className="h-5 w-5 text-[#FF9E38]" strokeWidth={2} />
            {t('v6Diag' as never)}
          </span>
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
          {checking && !report ? (
            <div className="flex flex-col items-center gap-3 py-10 text-[13px] text-white/50">
              <Loader2 className="h-6 w-6 v6-orb-spin text-[#FF9E38]" />
              {t('v6DiagChecking' as never)}
            </div>
          ) : error ? (
            <div className="py-8 text-center text-[12.5px] text-white/50">{error === 'dev' ? t('v6NoResults' as never) : error}</div>
          ) : report && meta && cause ? (
            <>
              {/* Verdict card */}
              <div
                className="flex items-start gap-3 rounded-[18px] border p-4"
                style={{ background: `${meta.color}12`, borderColor: `${meta.color}3a` }}
              >
                <meta.Icon className="mt-0.5 h-6 w-6 shrink-0" style={{ color: meta.color }} strokeWidth={2} />
                <div className="min-w-0">
                  <div className="text-[14.5px] font-semibold leading-tight text-white">{cause.title}</div>
                  <div className="mt-1 text-[12.5px] leading-snug text-white/65">{cause.summary}</div>
                </div>
              </div>

              {/* What to do */}
              {report.user_actions.length > 0 && (
                <div className="mt-3 rounded-[16px] border border-white/[0.08] bg-white/[0.04] p-3.5">
                  {report.user_actions.map((a, i) => (
                    <div key={i} className="flex items-start gap-2 py-0.5 text-[12.5px] leading-snug text-white/75">
                      <span className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-[#FF9E38]" />
                      {a}
                    </div>
                  ))}
                </div>
              )}

              {/* Failed checks (human line each) */}
              {problems.length > 0 && (
                <div className="mt-3">
                  {problems.slice(0, 6).map((c) => (
                    <div key={c.id + c.label} className="flex items-center gap-2 border-b border-white/[0.06] px-1 py-2">
                      <span className="h-[7px] w-[7px] shrink-0 rounded-full" style={{ background: CHECK_COLORS[c.status] }} />
                      <span className="min-w-0 flex-1 truncate text-[12.5px] text-white/75" title={c.user_text}>{c.user_text}</span>
                      <span className="shrink-0 text-[9.5px] uppercase tracking-wider text-white/30">{c.source}</span>
                    </div>
                  ))}
                </div>
              )}

              {/* Technical block, collapsed by default */}
              <button
                type="button"
                onClick={() => setTechOpen((o) => !o)}
                aria-expanded={techOpen}
                className="mt-3 flex w-full items-center gap-1.5 px-1 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-white/40 hover:text-white/65 v6-focus"
              >
                <ChevronDown className={`h-3.5 w-3.5 transition-transform ${techOpen ? '' : '-rotate-90'}`} strokeWidth={2.2} />
                {t('v6DiagForSupport' as never)}
              </button>
              {techOpen && (
                <div className="v6-glass-inset mt-1 max-h-[180px] overflow-y-auto rounded-[14px] p-3 font-mono text-[10.5px] leading-relaxed text-white/55">
                  <div className="break-words text-white/70">{report.support_summary}</div>
                  {report.checks.map((c) => (
                    <div key={`t-${c.id}-${c.label}`} className="mt-1.5 break-words">
                      <span style={{ color: CHECK_COLORS[c.status] }}>[{c.status}]</span> {c.id}: {c.technical_detail_redacted}
                    </div>
                  ))}
                </div>
              )}
            </>
          ) : null}
        </div>

        {/* Actions */}
        <div className="mt-4 flex shrink-0 flex-wrap gap-2">
          <button
            type="button"
            onClick={() => runDiagnosis()}
            disabled={checking || repairing}
            className="v6-hover-bright flex items-center gap-1.5 rounded-[12px] border border-white/[0.12] bg-white/[0.07] px-3 py-2 text-[12px] font-medium text-white v6-focus disabled:opacity-50"
          >
            {checking ? <Loader2 className="h-3.5 w-3.5 v6-orb-spin" /> : <Stethoscope className="h-3.5 w-3.5" strokeWidth={2.2} />}
            {t('v6DiagCheck' as never)}
          </button>
          {report?.can_auto_repair && (
            <button
              type="button"
              onClick={handleRepair}
              disabled={repairing || checking}
              className="flex items-center gap-1.5 rounded-[12px] px-3 py-2 text-[12px] font-semibold text-white transition-opacity hover:opacity-90 v6-focus disabled:opacity-60"
              style={{ background: 'linear-gradient(140deg, #FF9E38, #EA6D06)', boxShadow: '0 5px 14px rgba(234,109,6,0.3)' }}
            >
              {repairing ? <Loader2 className="h-3.5 w-3.5 v6-orb-spin" /> : <Wrench className="h-3.5 w-3.5" strokeWidth={2.2} />}
              {repairing ? t('v6DiagRepairing' as never) : t('v6DiagRepair' as never)}
            </button>
          )}
          <button
            type="button"
            onClick={handleCopy}
            disabled={!report}
            className="v6-hover-bright flex items-center gap-1.5 rounded-[12px] border border-white/[0.12] bg-white/[0.07] px-3 py-2 text-[12px] font-medium text-white v6-focus disabled:opacity-50"
          >
            {copied ? <Check className="h-3.5 w-3.5 text-[#3ddc84]" strokeWidth={2.4} /> : <Copy className="h-3.5 w-3.5" strokeWidth={2.2} />}
            {copied ? t('v6DiagCopied' as never) : t('v6DiagCopy' as never)}
          </button>
          {report?.bundle_available && (
            <button
              type="button"
              onClick={handleExport}
              disabled={exporting}
              title={t('v6DiagSaveFull' as never)}
              className="v6-hover-bright flex items-center gap-1.5 rounded-[12px] border border-white/[0.12] bg-white/[0.07] px-3 py-2 text-[12px] font-medium text-white v6-focus disabled:opacity-50"
            >
              {exporting ? <Loader2 className="h-3.5 w-3.5 v6-orb-spin" /> : <FileDown className="h-3.5 w-3.5" strokeWidth={2.2} />}
              {t('v6DiagSaveFull' as never)}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
