import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { open } from '@tauri-apps/plugin-shell';
import { api } from '@/lib/tauri';
import type { DebugInfo, FactoryResetReport } from '@/lib/types';
import { SettingRow } from './SettingRow';
import { useAppDispatch } from '@/store/hooks';
import { fetchConfig } from '@/store/slices/configSlice';
import { fetchStats } from '@/store/slices/statsSlice';
import { fetchWhisperModels, fetchLlmModels } from '@/store/slices/modelsSlice';

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const { t } = useTranslation();

  return (
    <button
      onClick={() => {
        navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      }}
      className="px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-all bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-600"
    >
      {copied ? t('debug.copied') : label}
    </button>
  );
}

function StorageBar({ info }: { info: DebugInfo }) {
  const { t } = useTranslation();

  const segments = [
    { key: 'whisper', label: t('debug.whisperModels'), bytes: info.whisper_models_bytes, color: 'bg-amber-500' },
    { key: 'llm', label: t('debug.llmModels'), bytes: info.llm_models_bytes, color: 'bg-purple-500' },
    { key: 'db', label: t('debug.database'), bytes: info.database_bytes, color: 'bg-blue-500' },
    { key: 'logs', label: t('debug.logs'), bytes: info.logs_bytes, color: 'bg-slate-400' },
    { key: 'config', label: t('debug.config'), bytes: info.config_bytes, color: 'bg-emerald-500' },
  ].filter((s) => s.bytes > 0);

  const total = segments.reduce((sum, s) => sum + s.bytes, 0);
  if (total === 0) return null;

  return (
    <div className="py-4 border-b border-slate-50 dark:border-slate-700">
      <div className="flex items-center justify-between mb-2">
        <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('debug.storage')}</p>
        <span className="text-slate-400 dark:text-slate-500 text-xs">{formatBytes(total)}</span>
      </div>
      <p className="text-slate-400 dark:text-slate-500 text-xs mb-3">{t('debug.storageDesc')}</p>

      {/* Stacked bar */}
      <div className="flex h-3 rounded-full overflow-hidden bg-slate-100 dark:bg-slate-700">
        {segments.map((s) => {
          const pct = Math.max((s.bytes / total) * 100, 2);
          return (
            <div
              key={s.key}
              className={`${s.color} transition-all`}
              style={{ width: `${pct}%` }}
              title={`${s.label}: ${formatBytes(s.bytes)}`}
            />
          );
        })}
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-x-4 gap-y-1 mt-2">
        {segments.map((s) => (
          <div key={s.key} className="flex items-center gap-1.5">
            <span className={`w-2.5 h-2.5 rounded-sm ${s.color}`} />
            <span className="text-xs text-slate-500 dark:text-slate-400">
              {s.label} <span className="font-mono">{formatBytes(s.bytes)}</span>
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function CodeBlock({ command, label }: { command: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const { t } = useTranslation();

  return (
    <div className="mb-2">
      <p className="text-slate-500 dark:text-slate-400 text-xs mb-1">{label}</p>
      <div className="flex items-center gap-2 bg-slate-900 dark:bg-slate-950 rounded-lg px-3 py-2">
        <code className="text-xs text-emerald-400 font-mono flex-1 select-all">{command}</code>
        <button
          onClick={() => {
            navigator.clipboard.writeText(command);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
          }}
          className="text-slate-500 hover:text-slate-300 transition-colors cursor-pointer flex-shrink-0"
        >
          <i className={`${copied ? 'ri-check-line text-emerald-400' : 'ri-file-copy-line'} text-sm`} />
        </button>
      </div>
    </div>
  );
}

type PillTone = 'green' | 'grey' | 'red' | 'blue';

function Pill({ tone, children }: { tone: PillTone; children: React.ReactNode }) {
  const tones: Record<PillTone, string> = {
    green: 'bg-emerald-100 dark:bg-emerald-500/20 text-emerald-700 dark:text-emerald-400',
    grey: 'bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400',
    red: 'bg-red-100 dark:bg-red-500/20 text-red-700 dark:text-red-400',
    blue: 'bg-sky-100 dark:bg-sky-500/20 text-sky-700 dark:text-sky-400',
  };
  return <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${tones[tone]}`}>{children}</span>;
}

function Row({ label, value, pill }: { label: string; value?: string; pill?: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between px-3 py-2 bg-slate-50 dark:bg-slate-900 rounded-lg">
      <div className="flex items-center gap-2 min-w-0">
        <span className="text-xs text-slate-600 dark:text-slate-300 flex-shrink-0">{label}</span>
        {value && <span className="text-xs font-mono text-slate-500 dark:text-slate-400 truncate">{value}</span>}
      </div>
      {pill}
    </div>
  );
}

function RuntimeBlock({ info }: { info: DebugInfo }) {
  const { t } = useTranslation();
  const { stt, pp } = info;

  let sttPill: React.ReactNode;
  if (!stt.is_local) {
    sttPill = <Pill tone="blue">{t('debug.cloud')}</Pill>;
  } else if (!stt.backend_ready) {
    sttPill = <Pill tone="grey">{t('debug.loading')}</Pill>;
  } else {
    sttPill = <Pill tone={stt.using_gpu ? 'green' : 'grey'}>{stt.using_gpu ? t('debug.gpuActive') : t('debug.cpuOnly')}</Pill>;
  }

  let ppContent: React.ReactNode;
  if (pp.kind === 'disabled') {
    ppContent = <Row label={t('debug.provider')} pill={<Pill tone="grey">{t('debug.disabled')}</Pill>} />;
  } else if (pp.kind === 'cloud') {
    ppContent = (
      <>
        <Row label={t('debug.provider')} value={pp.provider} pill={<Pill tone="blue">{t('debug.cloud')}</Pill>} />
        <Row label={t('debug.model')} value={pp.model} />
      </>
    );
  } else {
    const status = pp.ollama_status;
    const modeLabel = pp.kind === 'ollama_managed' ? t('debug.ollamaManaged') : t('debug.ollamaRemote');
    let pill: React.ReactNode;
    if (!status || !status.reachable) {
      pill = <Pill tone="red">{t('debug.ollamaUnreachable')}</Pill>;
    } else if (!status.model_loaded) {
      pill = <Pill tone="grey">{t('debug.modelNotLoaded')}</Pill>;
    } else {
      const vramMb = Math.round(status.vram_bytes / (1024 * 1024));
      pill = <Pill tone={status.using_gpu ? 'green' : 'grey'}>
        {status.using_gpu ? `${t('debug.gpuActive')} (${vramMb} MB)` : t('debug.cpuOnly')}
      </Pill>;
    }
    ppContent = (
      <>
        <Row label={t('debug.provider')} value={`Ollama · ${modeLabel}`} pill={pill} />
        <Row label={t('debug.model')} value={pp.model} />
      </>
    );
  }

  return (
    <div className="py-4 border-b border-slate-50 dark:border-slate-700">
      <div className="flex items-center justify-between mb-2">
        <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('debug.runtime')}</p>
        <span className="text-slate-400 dark:text-slate-500 text-xs">
          {t('debug.build')}: {info.gpu_backend.toUpperCase()}
        </span>
      </div>
      <p className="text-slate-400 dark:text-slate-500 text-xs mb-3">{t('debug.runtimeDesc')}</p>

      <div className="space-y-3">
        <div>
          <p className="text-slate-700 dark:text-slate-300 text-xs font-medium mb-1.5">{t('debug.stt')}</p>
          <div className="space-y-2">
            <Row
              label={`${stt.backend}${stt.model ? ` · ${stt.model}` : ''}`}
              pill={sttPill}
            />
          </div>
        </div>
        <div>
          <p className="text-slate-700 dark:text-slate-300 text-xs font-medium mb-1.5">{t('debug.postProcessing')}</p>
          <div className="space-y-2">{ppContent}</div>
        </div>
        {info.app_vram_mb != null && (
          <Row
            label={t('debug.appVram')}
            pill={<span className="text-xs font-mono font-medium text-slate-600 dark:text-slate-300">{info.app_vram_mb} MB</span>}
          />
        )}
      </div>
    </div>
  );
}

export default function DebugSection() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const dispatch = useAppDispatch();
  const [enabled, setEnabled] = useState(false);
  const [debugInfo, setDebugInfo] = useState<DebugInfo | null>(null);
  const [resetConfirming, setResetConfirming] = useState(false);
  const [resetBusy, setResetBusy] = useState(false);
  const [resetReport, setResetReport] = useState<FactoryResetReport | null>(null);
  const [resetError, setResetError] = useState<string | null>(null);

  const clearWebStorage = useCallback(() => {
    // localStorage holds card-order, preset snapshots, manual-balance overrides.
    // All of it represents user state that factory reset should also wipe.
    try { localStorage.clear(); } catch (e) { console.error('localStorage.clear failed', e); }
    try { sessionStorage.clear(); } catch (e) { console.error('sessionStorage.clear failed', e); }
  }, []);

  const factoryReset = useCallback(async () => {
    setResetBusy(true);
    setResetError(null);
    setResetReport(null);
    try {
      const report = await api.factoryReset();
      setResetReport(report);

      // Clear UI-side persistent state regardless of backend success — these
      // live in the webview's localStorage, not on disk, and there's no harm.
      clearWebStorage();

      // Refresh redux state so the UI reflects post-reset reality.
      await Promise.all([
        dispatch(fetchConfig()),
        dispatch(fetchStats()),
        dispatch(fetchWhisperModels()),
        dispatch(fetchLlmModels()),
      ]);

      if (report.success) {
        // Tiny delay so the user sees the green "all clear" before redirect.
        setTimeout(() => navigate('/onboarding', { replace: true }), 600);
      } else {
        // Partial failure: keep the dialog open and surface what failed so the
        // user can manually intervene rather than silently leaving leftovers.
        setResetBusy(false);
      }
    } catch (e) {
      console.error('factory_reset failed', e);
      setResetError(String(e));
      setResetBusy(false);
    }
  }, [dispatch, navigate, clearWebStorage]);

  const fetchDebugInfo = useCallback(() => {
    api.getDebugInfo().then(setDebugInfo).catch(console.error);
  }, []);

  useEffect(() => {
    if (!enabled) {
      setDebugInfo(null);
      return;
    }

    fetchDebugInfo();
    const interval = setInterval(fetchDebugInfo, 3000);
    return () => clearInterval(interval);
  }, [enabled, fetchDebugInfo]);

  const emailLogs = () => {
    if (!debugInfo) return;
    const subject = encodeURIComponent('Verbatim Debug Logs');
    const body = encodeURIComponent(
      `Log files are located at: ${debugInfo.log_dir}\n\nPlease attach the most recent log file(s) from that directory.`
    );
    open(`mailto:me@mkamran.us?subject=${subject}&body=${body}`);
  };

  const openLogFolder = () => {
    if (!debugInfo) return;
    api.openPath(debugInfo.log_dir).catch((e) => {
      console.error('open_path failed', e);
    });
  };

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
      {/* Header with toggle */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center">
            <i className="ri-bug-line text-amber-500 text-base" />
          </div>
          <div>
            <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('debug.title')}</h3>
            <p className="text-slate-400 dark:text-slate-500 text-xs">{t('debug.description')}</p>
          </div>
        </div>
        <button
          onClick={() => setEnabled(!enabled)}
          className={`w-10 h-5 rounded-full transition-all relative flex-shrink-0 cursor-pointer ${enabled ? 'bg-amber-500' : 'bg-slate-200 dark:bg-slate-600'}`}
        >
          <span
            className="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all"
            style={{ left: enabled ? '22px' : '2px' }}
          />
        </button>
      </div>

      {/* Content — only when enabled */}
      {enabled && debugInfo && (
        <div className="mt-5 border-t border-slate-100 dark:border-slate-700 pt-1">
          {/* Email Logs */}
          <SettingRow label={t('debug.emailLogs')} description={t('debug.emailLogsDesc')}>
            <button
              onClick={emailLogs}
              className="px-4 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-all bg-amber-500 hover:bg-amber-600 text-white"
            >
              <i className="ri-mail-send-line mr-1.5" />
              {t('debug.emailButton')}
            </button>
          </SettingRow>

          {/* Log Location */}
          <SettingRow label={t('debug.logLocation')} description={t('debug.logLocationDesc')}>
            <div className="flex items-center gap-2">
              <CopyButton text={debugInfo.log_dir} label={t('debug.copyPath')} />
              <button
                onClick={openLogFolder}
                className="px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-all bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-600"
              >
                <i className="ri-folder-open-line mr-1" />
                {t('debug.openFolder')}
              </button>
            </div>
          </SettingRow>

          {/* Log path display */}
          <div className="py-2 px-3 bg-slate-50 dark:bg-slate-900 rounded-lg mb-1 -mt-2">
            <code className="text-xs text-slate-500 dark:text-slate-400 font-mono break-all">{debugInfo.log_dir}</code>
          </div>

          {/* Storage */}
          <StorageBar info={debugInfo} />

          {/* Runtime */}
          <RuntimeBlock info={debugInfo} />

          {/* Memory */}
          <SettingRow label={t('debug.memory')} description={t('debug.memoryDesc')}>
            <div className="text-right">
              <p className="text-xs font-mono text-slate-600 dark:text-slate-300">
                {t('debug.processRam')}: {debugInfo.process_rss_mb} MB / {debugInfo.total_ram_mb.toLocaleString()} MB
              </p>
              {debugInfo.vram_info ? (
                <p className="text-xs font-mono text-slate-500 dark:text-slate-400 mt-0.5">
                  {t('debug.vram')}: {debugInfo.vram_info.used_mb} MB / {debugInfo.vram_info.total_mb.toLocaleString()} MB
                  <span className="text-slate-400 dark:text-slate-500 ml-1">({debugInfo.vram_info.gpu_name})</span>
                </p>
              ) : !debugInfo.amd_vram_info ? (
                <p className="text-xs text-slate-400 dark:text-slate-500 mt-0.5">{t('debug.vramNotAvailable')}</p>
              ) : null}
              {debugInfo.amd_vram_info && (
                <p className="text-xs font-mono text-slate-500 dark:text-slate-400 mt-0.5">
                  {t('debug.vram')}: {debugInfo.amd_vram_info.used_mb} MB / {debugInfo.amd_vram_info.total_mb.toLocaleString()} MB
                  <span className="text-slate-400 dark:text-slate-500 ml-1">({debugInfo.amd_vram_info.gpu_name})</span>
                </p>
              )}
            </div>
          </SettingRow>

          {/* Monitoring Commands */}
          <div className="py-4">
            <p className="text-slate-800 dark:text-slate-200 text-sm font-medium mb-1">{t('debug.monitoring')}</p>
            <p className="text-slate-400 dark:text-slate-500 text-xs mb-3">{t('debug.monitoringDesc')}</p>

            <CodeBlock
              label={t('debug.cpuMonitoring')}
              command="htop -p $(pgrep -d, verbatim)"
            />
            {debugInfo.vram_info && (
              <>
                <CodeBlock
                  label={t('debug.gpuMonitoringNvidia')}
                  command="watch -n1 nvidia-smi"
                />
                <CodeBlock
                  label="nvtop"
                  command="nvtop"
                />
              </>
            )}
            {debugInfo.amd_vram_info && (
              <CodeBlock
                label={t('debug.gpuMonitoringAmd')}
                command="watch -n1 rocm-smi"
              />
            )}
          </div>

          {/* Danger zone — Factory Reset */}
          <div className="mt-2 pt-4 border-t border-red-100 dark:border-red-500/20">
            <div className="flex items-start gap-3 mb-3">
              <div className="w-8 h-8 rounded-lg bg-red-50 dark:bg-red-500/10 flex items-center justify-center flex-shrink-0">
                <i className="ri-alert-line text-red-500 text-base" />
              </div>
              <div className="min-w-0">
                <p className="text-slate-900 dark:text-slate-100 font-semibold text-sm">
                  {t('debug.factoryReset')}
                </p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">
                  {t('debug.factoryResetDesc')}
                </p>
              </div>
            </div>

            {!resetConfirming && !resetBusy && !resetReport && !resetError && (
              <button
                onClick={() => setResetConfirming(true)}
                className="px-4 py-2 rounded-lg text-xs font-semibold cursor-pointer transition-all bg-red-500 hover:bg-red-600 text-white"
              >
                <i className="ri-delete-bin-line mr-1.5" />
                {t('debug.factoryResetButton')}
              </button>
            )}

            {resetConfirming && !resetBusy && !resetReport && !resetError && (
              <div className="rounded-lg border border-red-200 dark:border-red-500/30 bg-red-50/60 dark:bg-red-500/5 p-3">
                <p className="text-xs text-red-700 dark:text-red-300 mb-3 leading-relaxed">
                  {t('debug.factoryResetConfirm')}
                </p>
                <div className="flex items-center gap-2">
                  <button
                    onClick={factoryReset}
                    className="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition-all bg-red-600 hover:bg-red-700 text-white"
                  >
                    <i className="ri-delete-bin-line mr-1.5" />
                    {t('debug.factoryResetConfirmButton')}
                  </button>
                  <button
                    onClick={() => setResetConfirming(false)}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-all bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300"
                  >
                    {t('common.cancel')}
                  </button>
                </div>
              </div>
            )}

            {resetBusy && (
              <div className="rounded-lg border border-red-200 dark:border-red-500/30 bg-red-50/60 dark:bg-red-500/5 p-4">
                <div className="flex items-center gap-3">
                  <i className="ri-loader-4-line animate-spin text-red-500 text-xl" />
                  <div className="flex-1">
                    <p className="text-sm font-semibold text-red-700 dark:text-red-300">
                      {t('debug.factoryResetBusy')}
                    </p>
                    <p className="text-xs text-red-600/80 dark:text-red-400/80 mt-0.5">
                      {t('debug.factoryResetBusyDesc')}
                    </p>
                  </div>
                </div>
              </div>
            )}

            {resetReport && (
              <div
                className={`rounded-lg border p-3 ${
                  resetReport.success
                    ? 'border-emerald-200 dark:border-emerald-500/30 bg-emerald-50/60 dark:bg-emerald-500/5'
                    : 'border-amber-200 dark:border-amber-500/30 bg-amber-50/60 dark:bg-amber-500/5'
                }`}
              >
                <div className="flex items-start gap-2 mb-3">
                  <i
                    className={
                      resetReport.success
                        ? 'ri-checkbox-circle-fill text-emerald-500 text-lg'
                        : 'ri-error-warning-fill text-amber-500 text-lg'
                    }
                  />
                  <div className="min-w-0 flex-1">
                    <p
                      className={`text-sm font-semibold ${
                        resetReport.success
                          ? 'text-emerald-700 dark:text-emerald-300'
                          : 'text-amber-700 dark:text-amber-300'
                      }`}
                    >
                      {resetReport.success
                        ? t('debug.factoryResetDone')
                        : t('debug.factoryResetPartial')}
                    </p>
                    <p className="text-xs text-slate-600 dark:text-slate-400 mt-0.5">
                      {resetReport.success
                        ? t('debug.factoryResetDoneDesc')
                        : t('debug.factoryResetPartialDesc')}
                    </p>
                  </div>
                </div>
                <ul className="space-y-1 max-h-48 overflow-y-auto">
                  {resetReport.steps.map((s) => (
                    <li key={s.name} className="flex items-start gap-2 text-xs font-mono">
                      <i
                        className={
                          s.ok
                            ? 'ri-check-line text-emerald-500 mt-0.5'
                            : 'ri-close-line text-red-500 mt-0.5'
                        }
                      />
                      <div className="min-w-0 flex-1">
                        <span className={s.ok ? 'text-slate-600 dark:text-slate-300' : 'text-red-700 dark:text-red-300 font-semibold'}>
                          {s.name}
                        </span>
                        {s.detail && (
                          <p className="text-slate-400 dark:text-slate-500 text-[10px] break-words mt-0.5">
                            {s.detail}
                          </p>
                        )}
                      </div>
                    </li>
                  ))}
                </ul>
                {!resetReport.success && (
                  <button
                    onClick={() => {
                      setResetReport(null);
                      setResetConfirming(false);
                    }}
                    className="mt-3 px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300"
                  >
                    {t('common.cancel')}
                  </button>
                )}
              </div>
            )}

            {resetError && !resetReport && (
              <div className="rounded-lg border border-red-300 dark:border-red-500/40 bg-red-50 dark:bg-red-500/10 p-3">
                <div className="flex items-start gap-2">
                  <i className="ri-close-circle-fill text-red-500 text-lg" />
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-semibold text-red-700 dark:text-red-300">
                      {t('debug.factoryResetFailed')}
                    </p>
                    <p className="text-xs font-mono text-red-600 dark:text-red-400 mt-1 break-words">
                      {resetError}
                    </p>
                  </div>
                </div>
                <button
                  onClick={() => {
                    setResetError(null);
                    setResetConfirming(false);
                  }}
                  className="mt-3 px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300"
                >
                  {t('common.cancel')}
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Loading state when enabled but data not yet loaded */}
      {enabled && !debugInfo && (
        <div className="mt-5 border-t border-slate-100 dark:border-slate-700 pt-5 flex items-center justify-center py-8">
          <i className="ri-loader-4-line animate-spin text-slate-400 text-xl" />
        </div>
      )}
    </div>
  );
}
