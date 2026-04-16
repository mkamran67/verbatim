import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-shell';
import { api } from '@/lib/tauri';
import type { DebugInfo } from '@/lib/types';
import { SettingRow } from './SettingRow';

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

export default function DebugSection() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [debugInfo, setDebugInfo] = useState<DebugInfo | null>(null);

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
    open(debugInfo.log_dir);
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

          {/* GPU Status */}
          <div className="py-4 border-b border-slate-50 dark:border-slate-700">
            <div className="flex items-center justify-between mb-2">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('debug.gpuStatus')}</p>
              <span className="text-slate-400 dark:text-slate-500 text-xs">{t('debug.gpuBackend')}: {debugInfo.gpu_backend.toUpperCase()}</span>
            </div>
            <p className="text-slate-400 dark:text-slate-500 text-xs mb-3">{t('debug.gpuStatusDesc')}</p>

            <div className="space-y-2">
              <div className="flex items-center justify-between px-3 py-2 bg-slate-50 dark:bg-slate-900 rounded-lg">
                <span className="text-xs text-slate-600 dark:text-slate-300">{t('debug.sttGpu')}</span>
                <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${debugInfo.stt_using_gpu ? 'bg-emerald-100 dark:bg-emerald-500/20 text-emerald-700 dark:text-emerald-400' : 'bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400'}`}>
                  {debugInfo.stt_using_gpu ? t('debug.gpuActive') : t('debug.cpuOnly')}
                </span>
              </div>
              <div className="flex items-center justify-between px-3 py-2 bg-slate-50 dark:bg-slate-900 rounded-lg">
                <span className="text-xs text-slate-600 dark:text-slate-300">{t('debug.llmGpu')}</span>
                <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${debugInfo.llm_using_gpu ? 'bg-emerald-100 dark:bg-emerald-500/20 text-emerald-700 dark:text-emerald-400' : 'bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400'}`}>
                  {debugInfo.llm_using_gpu ? t('debug.gpuActive') : t('debug.cpuOnly')}
                </span>
              </div>
              {debugInfo.app_vram_mb != null && (
                <div className="flex items-center justify-between px-3 py-2 bg-slate-50 dark:bg-slate-900 rounded-lg">
                  <span className="text-xs text-slate-600 dark:text-slate-300">{t('debug.appVram')}</span>
                  <span className="text-xs font-mono font-medium text-slate-600 dark:text-slate-300">
                    {debugInfo.app_vram_mb} MB
                  </span>
                </div>
              )}
            </div>
          </div>

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
