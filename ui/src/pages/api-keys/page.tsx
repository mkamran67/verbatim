import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import { open } from '@tauri-apps/plugin-shell';
import type { Config, SystemInfo } from '@/lib/types';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { fetchWhisperModels, fetchLlmModels } from '@/store/slices/modelsSlice';
import { scoreCompatibility, type Tier } from '@/lib/compatibility';
import { WHISPER_META } from '@/lib/whisper-catalog';
import { OLLAMA_CATALOG } from '@/lib/ollama-catalog';
import { estimateLlmTokensPerSec, LLM_MIN_TOK_S } from '@/lib/throughput';

export default function ApiKeys() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const storeConfig = useAppSelector((s) => s.config.data);
  const models = useAppSelector((s) => s.models.whisperModels);
  const llmModels = useAppSelector((s) => s.models.llmModels);
  const downloadProgress = useAppSelector((s) => s.models.downloadProgress);
  const llmDownloadProgress = useAppSelector((s) => s.models.llmDownloadProgress);

  const [config, setConfig] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);

  useEffect(() => {
    if (storeConfig && !config) {
      setConfig(structuredClone(storeConfig));
    }
  }, [storeConfig]);

  useEffect(() => {
    api.getSystemInfo().then(setSysInfo).catch(() => {});
  }, []);

  // Score every Whisper model and filter to those that comfortably fit.
  const whisperScores = (() => {
    if (!sysInfo) return new Map<string, Tier>();
    const out = new Map<string, Tier>();
    for (const [name, meta] of Object.entries(WHISPER_META)) {
      const r = scoreCompatibility({
        total_ram_mb: sysInfo.total_ram_mb,
        cpu_cores: sysInfo.cpu_cores,
        platform: sysInfo.platform,
        min_ram_mb: meta.working_set_mb,
        recommended_ram_mb: meta.working_set_mb,
        recommended_cores: meta.recommended_cores,
      });
      out.set(name, r.tier);
    }
    return out;
  })();
  const whisperOrder = ['large-v3', 'medium', 'medium.en', 'small', 'small.en', 'base', 'base.en', 'tiny', 'tiny.en'];
  const recommendedModels = whisperOrder.filter((n) => {
    const t = whisperScores.get(n);
    return t === 'best' || t === 'fits';
  });
  const bestModel = recommendedModels[0] ?? null;

  // LLM recommendations: score the curated Ollama catalog with the same rules.
  const llmRecommended = (() => {
    if (!sysInfo) return [];
    return OLLAMA_CATALOG
      .filter((e) => {
        const tps = estimateLlmTokensPerSec(e, sysInfo, 0);
        const r = scoreCompatibility({
          total_ram_mb: sysInfo.total_ram_mb,
          cpu_cores: sysInfo.cpu_cores,
          platform: sysInfo.platform,
          min_ram_mb: e.min_ram_mb,
          recommended_ram_mb: e.recommended_ram_mb,
          recommended_cores: e.recommended_cores,
          estimated_throughput_tok_s: tps ?? undefined,
          min_throughput_tok_s: LLM_MIN_TOK_S,
        });
        return r.tier === 'best' || r.tier === 'fits';
      })
      .sort((a, b) => b.recommended_ram_mb - a.recommended_ram_mb)
      .map((e) => e.tag);
  })();
  const bestLlmModel = llmRecommended[0] ?? null;

  const update = (fn: (c: Config) => void) => {
    if (!config) return;
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await dispatch(saveConfig(config)).unwrap();
      setSaved(true);
    } catch (e) {
      console.error('Failed to save:', e);
    }
    setSaving(false);
  };

  if (!config) {
    return (
      <Layout title={t('apiKeys.title')} subtitle="Loading...">
        <div className="flex items-center justify-center py-20">
          <i className="ri-loader-4-line animate-spin text-slate-400 text-2xl" />
        </div>
      </Layout>
    );
  }

  return (
    <Layout title={t('apiKeys.title')} subtitle={t('apiKeys.subtitle')}>
      <div className="flex flex-col gap-5 max-w-[1000px]">

        {/* API Keys */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('apiKeys.heading')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('apiKeys.headingDesc')}</p>

          <div className="flex flex-wrap items-center justify-between gap-2 py-4 border-b border-slate-50 dark:border-slate-700">
            <div className="flex-1 pr-8">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('apiKeys.openaiKey')}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiKeys.openaiKeyDesc')}{' '}
                <a href="#" onClick={(e) => { e.preventDefault(); open('https://platform.openai.com/api-keys'); }} className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 inline-flex items-center gap-0.5">{t('apiKeys.getKey')} <i className="ri-external-link-line text-[10px]" /></a>
              </p>
            </div>
            <input
              type="password"
              value={config.openai.api_key}
              onChange={(e) => update((c) => { c.openai.api_key = e.target.value; })}
              placeholder="sk-..."
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none w-full sm:w-64 font-mono"
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 py-4 border-b border-slate-50 dark:border-slate-700">
            <div className="flex-1 pr-8">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('apiKeys.openaiAdminKey')}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiKeys.openaiAdminKeyDesc')}{' '}
                <a href="#" onClick={(e) => { e.preventDefault(); open('https://platform.openai.com/settings/organization/admin-keys'); }} className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 inline-flex items-center gap-0.5">{t('apiKeys.getKey')} <i className="ri-external-link-line text-[10px]" /></a>
              </p>
            </div>
            <input
              type="password"
              value={config.openai.admin_key}
              onChange={(e) => update((c) => { c.openai.admin_key = e.target.value; })}
              placeholder="sk-admin-..."
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none w-full sm:w-64 font-mono"
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 py-4 border-b border-slate-50 dark:border-slate-700">
            <div className="flex-1 pr-8">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('apiKeys.deepgramKey')}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiKeys.deepgramKeyDesc')}{' '}
                <a href="#" onClick={(e) => { e.preventDefault(); open('https://console.deepgram.com/'); }} className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 inline-flex items-center gap-0.5">{t('apiKeys.getKey')} <i className="ri-external-link-line text-[10px]" /></a>
              </p>
            </div>
            <input
              type="password"
              value={config.deepgram.api_key}
              onChange={(e) => update((c) => { c.deepgram.api_key = e.target.value; })}
              placeholder="API key..."
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none w-full sm:w-64 font-mono"
            />
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 py-4">
            <div className="flex-1 pr-8">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('apiKeys.smallestKey')}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiKeys.smallestKeyDesc')}{' '}
                <a href="#" onClick={(e) => { e.preventDefault(); open('https://app.smallest.ai/dashboard'); }} className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 inline-flex items-center gap-0.5">{t('apiKeys.getKey')} <i className="ri-external-link-line text-[10px]" /></a>
              </p>
            </div>
            <input
              type="password"
              value={config.smallest.api_key}
              onChange={(e) => update((c) => { c.smallest.api_key = e.target.value; })}
              placeholder="API key..."
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none w-full sm:w-64 font-mono"
            />
          </div>

          <div className="flex items-center justify-end gap-3 mt-3">
            {saved && <span className="text-emerald-500 text-xs font-medium">{t('common.saved')}!</span>}
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-4 py-2 rounded-lg text-sm font-medium bg-amber-500 hover:bg-amber-600 text-white cursor-pointer transition-all whitespace-nowrap disabled:opacity-50"
            >
              {saving ? t('apiKeys.saving') : t('apiKeys.saveKeys')}
            </button>
          </div>
        </div>

        {/* Available Models */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('apiKeys.availableModels')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('apiKeys.availableModelsDesc')}</p>

          <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
            {models.map((m) => {
              const isDownloading = downloadProgress?.model === m.name;
              const pct = isDownloading && downloadProgress.total > 0
                ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100)
                : 0;
              const sizeMb = (m.size_bytes / 1_000_000).toFixed(0);
              const isBest = bestModel === m.name;
              const isRecommended = recommendedModels.includes(m.name);

              return (
                <div key={m.name} className="flex items-center justify-between py-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <span className="text-slate-800 dark:text-slate-200 text-sm font-medium font-mono">{m.name}</span>
                    <span className="text-slate-400 dark:text-slate-500 text-xs">{sizeMb} MB</span>
                    {m.downloaded && <span className="text-emerald-500 text-xs font-medium flex items-center gap-1"><i className="ri-check-line text-xs" />{t('common.downloaded')}</span>}
                    {isBest && sysInfo && (
                      <span className="relative group inline-flex items-center gap-1 text-amber-600 dark:text-amber-400 text-[10px] font-semibold bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-full px-2 py-0.5">
                        <i className="ri-star-fill text-[10px]" />{t('common.recommended')}
                        <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2.5 py-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                          {t('apiKeys.bestFor', { ram: (sysInfo.total_ram_mb / 1024).toFixed(0), cores: sysInfo.cpu_cores })}
                        </span>
                      </span>
                    )}
                    {!isBest && isRecommended && sysInfo && (
                      <span className="relative group inline-flex items-center gap-1 text-slate-500 dark:text-slate-400 text-[10px] font-medium">
                        <i className="ri-check-double-line text-[10px]" />{t('common.compatible')}
                        <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2.5 py-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                          {t('apiKeys.suitableFor', { ram: (sysInfo.total_ram_mb / 1024).toFixed(0), cores: sysInfo.cpu_cores })}
                        </span>
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-2 flex-shrink-0">
                    {isDownloading ? (
                      <>
                        <div className="w-36 flex items-center gap-2">
                          {downloadProgress.verifying ? (
                            <>
                              <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                                <div className="h-full bg-sky-400 rounded-full animate-pulse" style={{ width: '100%' }} />
                              </div>
                              <span className="text-sky-500 text-[10px] whitespace-nowrap">{t('common.verifying')}</span>
                            </>
                          ) : (
                            <>
                              <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                                <div className="h-full bg-amber-400 rounded-full transition-all" style={{ width: `${pct}%` }} />
                              </div>
                              <span className="text-slate-500 dark:text-slate-400 text-[10px] tabular-nums w-8 text-right">{pct}%</span>
                            </>
                          )}
                        </div>
                        <button
                          onClick={() => api.cancelModelDownload()}
                          disabled={downloadProgress.verifying}
                          className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                        >
                          {t('common.cancel')}
                        </button>
                      </>
                    ) : m.downloaded ? (
                      <button
                        onClick={() => api.deleteModel(m.name).then(() => dispatch(fetchWhisperModels()))}
                        className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all"
                      >
                        {t('common.delete')}
                      </button>
                    ) : (
                      <button
                        onClick={() => api.downloadModel(m.name)}
                        disabled={downloadProgress !== null}
                        className="px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 border border-amber-200 dark:border-amber-500/30 rounded-lg cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        {t('common.download')}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* LLM Models */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('apiKeys.llmModels')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('apiKeys.llmModelsDesc')}</p>

          {llmModels.length > 0 && !llmModels.some((m) => m.downloaded) && (
            <p className="text-amber-600/80 dark:text-amber-400/70 text-[11px] italic mb-3 flex items-center gap-1.5">
              <i className="ri-information-line text-[11px]" />
              {t('apiKeys.noLlmModelsHint')}
            </p>
          )}

          <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
            {llmModels.map((m) => {
              const isDownloading = llmDownloadProgress?.model === m.id;
              const pct = isDownloading && llmDownloadProgress.total > 0
                ? Math.round((llmDownloadProgress.downloaded / llmDownloadProgress.total) * 100)
                : 0;
              const sizeMb = m.size_bytes >= 1_000_000_000
                ? `${(m.size_bytes / 1_000_000_000).toFixed(1)} GB`
                : `${(m.size_bytes / 1_000_000).toFixed(0)} MB`;
              const isBest = bestLlmModel === m.id;
              const isRecommended = llmRecommended.includes(m.id);

              return (
                <div key={m.id} className="flex items-center justify-between py-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <span className="text-slate-800 dark:text-slate-200 text-sm font-medium">{m.display_name}</span>
                    <span className="text-slate-400 dark:text-slate-500 text-xs">{sizeMb}</span>
                    {m.downloaded && <span className="text-emerald-500 text-xs font-medium flex items-center gap-1"><i className="ri-check-line text-xs" />{t('common.downloaded')}</span>}
                    {isBest && sysInfo && (
                      <span className="relative group inline-flex items-center gap-1 text-amber-600 dark:text-amber-400 text-[10px] font-semibold bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-full px-2 py-0.5">
                        <i className="ri-star-fill text-[10px]" />{t('common.recommended')}
                        <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2.5 py-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                          {t('apiKeys.bestForRam', { ram: (sysInfo.total_ram_mb / 1024).toFixed(0) })}
                        </span>
                      </span>
                    )}
                    {!isBest && isRecommended && sysInfo && (
                      <span className="relative group inline-flex items-center gap-1 text-slate-500 dark:text-slate-400 text-[10px] font-medium">
                        <i className="ri-check-double-line text-[10px]" />{t('common.compatible')}
                        <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2.5 py-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                          {t('apiKeys.suitableForRam', { ram: (sysInfo.total_ram_mb / 1024).toFixed(0) })}
                        </span>
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-2 flex-shrink-0">
                    {isDownloading ? (
                      <>
                        <div className="w-36 flex items-center gap-2">
                          {llmDownloadProgress.verifying ? (
                            <>
                              <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                                <div className="h-full bg-sky-400 rounded-full animate-pulse" style={{ width: '100%' }} />
                              </div>
                              <span className="text-sky-500 text-[10px] whitespace-nowrap">{t('common.verifying')}</span>
                            </>
                          ) : (
                            <>
                              <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                                <div className="h-full bg-amber-400 rounded-full transition-all" style={{ width: `${pct}%` }} />
                              </div>
                              <span className="text-slate-500 dark:text-slate-400 text-[10px] tabular-nums w-8 text-right">{pct}%</span>
                            </>
                          )}
                        </div>
                        <button
                          onClick={() => api.cancelLlmModelDownload()}
                          disabled={llmDownloadProgress.verifying}
                          className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                        >
                          {t('common.cancel')}
                        </button>
                      </>
                    ) : m.downloaded ? (
                      <button
                        onClick={() => api.deleteLlmModel(m.id).then(() => dispatch(fetchLlmModels()))}
                        className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all"
                      >
                        {t('common.delete')}
                      </button>
                    ) : (
                      <button
                        onClick={() => api.downloadLlmModel(m.id)}
                        disabled={llmDownloadProgress !== null}
                        className="px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 border border-amber-200 dark:border-amber-500/30 rounded-lg cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        {t('common.download')}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </Layout>
  );
}
