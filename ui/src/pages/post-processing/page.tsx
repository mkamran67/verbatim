import { useState, useEffect, useRef } from 'react';
import { listen as listenTauriEvent } from '@tauri-apps/api/event';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import type { Config } from '@/lib/types';
import Select from '@/components/ui/Select';
import { SettingRow, Toggle } from '../settings/components/SettingRow';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { fetchLlmModels } from '@/store/slices/modelsSlice';
import { DEFAULT_PP_PROMPT, SEED_PROMPTS } from '@/lib/prompts';
import EmojiPicker from '@/components/ui/EmojiPicker';
import {
  OLLAMA_CATALOG, searchCatalog, fmtSize,
  type OllamaCatalogEntry, type OllamaCategory,
} from '@/lib/ollama-catalog';
import { scoreCompatibility, type ScoreResult, type Tier } from '@/lib/compatibility';
import { whisperWorkingSetMb } from '@/lib/whisper-catalog';
import { estimateLlmTokensPerSec, fmtTokensPerSec, LLM_MIN_TOK_S } from '@/lib/throughput';

export default function PostProcessing() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const dispatch = useAppDispatch();
  const storeConfig = useAppSelector((s) => s.config.data);
  const llmModels = useAppSelector((s) => s.models.llmModels);
  const llmDownloadProgress = useAppSelector((s) => s.models.llmDownloadProgress);

  const [config, setConfig] = useState<Config | null>(null);
  const [saved, setSaved] = useState(false);
  const [keyWarning, setKeyWarning] = useState<string | null>(null);
  const [promptModal, setPromptModal] = useState(false);
  const [savingPromptName, setSavingPromptName] = useState(false);
  const [promptNameDraft, setPromptNameDraft] = useState('');
  const [llmModelPrompt, setLlmModelPrompt] = useState<{ id: string; displayName: string; downloading: boolean } | null>(null);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const ollamaSectionRef = useRef<HTMLDivElement | null>(null);

  const scrollToOllamaSection = () => {
    ollamaSectionRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  useEffect(() => {
    const el = promptTextareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
  }, [config?.post_processing.prompt]);
  // null = closed, '__default__' = editing default, '__new__' = creating new, string = editing saved by name
  const [editingPrompt, setEditingPrompt] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editBody, setEditBody] = useState('');
  const [editEmoji, setEditEmoji] = useState('📝');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [seeded, setSeeded] = useState(false);

  useEffect(() => {
    if (storeConfig) setConfig(structuredClone(storeConfig));
  }, [storeConfig]);

  // Seed Casual and Formal prompts on first launch
  useEffect(() => {
    if (!config || seeded) return;
    setSeeded(true);
    if (config.post_processing.saved_prompts.length === 0) {
      update((c) => {
        c.post_processing.saved_prompts = [...SEED_PROMPTS];
      });
    }
  }, [config]);

  useEffect(() => {
    if (!llmDownloadProgress && llmModelPrompt?.downloading) {
      setLlmModelPrompt(null);
    }
  }, [llmDownloadProgress]);

  // Auto-select the first downloaded Ollama model when none is selected
  // (or the previously-selected one is no longer downloaded).
  useEffect(() => {
    if (!config) return;
    if (config.post_processing.provider !== 'ollama') return;
    const downloaded = llmModels.filter((m) => m.downloaded);
    if (downloaded.length === 0) return;
    const current = config.post_processing.ollama_model;
    if (!current || !downloaded.some((m) => m.id === current)) {
      update((c) => { c.post_processing.ollama_model = downloaded[0].id; });
    }
  }, [llmModels, config?.post_processing.provider]);

  const update = (fn: (c: Config) => void) => {
    if (!config) return;
    // Cancel any pending debounced prompt save so it can't land after this
    // immediate save and overwrite it (e.g. preset switch mid-edit).
    if (promptSaveTimer.current) {
      window.clearTimeout(promptSaveTimer.current);
      promptSaveTimer.current = null;
      pendingPromptConfig.current = null;
    }
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
    dispatch(saveConfig(next)).then(() => setSaved(true)).catch(console.error);
  };

  // Debounced variant for high-frequency text input (e.g. system prompt
  // textarea). Updates local React state immediately so the textarea feels
  // responsive, but coalesces backend saves: every save is delayed
  // PROMPT_SAVE_DEBOUNCE_MS, and any earlier pending save is cancelled.
  // Pending save is flushed on unmount and on tab switch so changes can't
  // be lost.
  const PROMPT_SAVE_DEBOUNCE_MS = 600;
  const promptSaveTimer = useRef<number | null>(null);
  const pendingPromptConfig = useRef<Config | null>(null);

  const flushPromptSave = () => {
    if (promptSaveTimer.current) {
      window.clearTimeout(promptSaveTimer.current);
      promptSaveTimer.current = null;
    }
    if (pendingPromptConfig.current) {
      const c = pendingPromptConfig.current;
      pendingPromptConfig.current = null;
      dispatch(saveConfig(c)).then(() => setSaved(true)).catch(console.error);
    }
  };

  const updateDebounced = (fn: (c: Config) => void) => {
    if (!config) return;
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
    pendingPromptConfig.current = next;
    if (promptSaveTimer.current) window.clearTimeout(promptSaveTimer.current);
    promptSaveTimer.current = window.setTimeout(() => {
      promptSaveTimer.current = null;
      const c = pendingPromptConfig.current;
      if (!c) return;
      pendingPromptConfig.current = null;
      dispatch(saveConfig(c)).then(() => setSaved(true)).catch(console.error);
    }, PROMPT_SAVE_DEBOUNCE_MS);
  };

  useEffect(() => {
    // Flush on unmount so navigating away mid-edit doesn't drop changes.
    return () => { flushPromptSave(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    // Flush before the tab/window goes away.
    const onBeforeUnload = () => { flushPromptSave(); };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (!config) {
    return (
      <Layout title={t('pp.title')} subtitle="Loading...">
        <div className="flex items-center justify-center py-20">
          <i className="ri-loader-4-line animate-spin text-slate-400 text-2xl" />
        </div>
      </Layout>
    );
  }

  return (
    <Layout title={t('pp.title')} subtitle={t('pp.subtitle')}>
      <div className="max-w-[860px] flex flex-col gap-5 pb-12">
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('pp.heading')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('pp.headingDesc')}</p>

          {config.general.backend === 'deepgram' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-blue-50 dark:bg-blue-500/10 border border-blue-200 dark:border-blue-500/30 rounded-lg mb-4">
              <i className="ri-information-line text-blue-500" />
              <p className="text-blue-700 dark:text-blue-400 text-xs flex-1">
                {t('pp.deepgramNote')}
              </p>
            </div>
          )}

          <SettingRow label={t('common.enabled')} description={t('pp.enabledDesc')}>
            <Toggle
              on={config.post_processing.enabled}
              onChange={(v) => {
                if (v && config.post_processing.provider === 'openai' && !config.openai.api_key) {
                  setKeyWarning('post-processing');
                  return;
                }
                if (v && config.post_processing.provider === 'ollama' && !llmModels.some((m) => m.downloaded)) {
                  setKeyWarning('no-llm-model');
                  return;
                }
                setKeyWarning(null);
                update((c) => { c.post_processing.enabled = v; });
              }}
            />
          </SettingRow>

          {keyWarning === 'post-processing' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mb-2">
              <i className="ri-key-line text-amber-500" />
              <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('pp.openaiKeyRequired')}</p>
              <button
                onClick={() => { setKeyWarning(null); navigate('/api-keys'); }}
                className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 underline cursor-pointer whitespace-nowrap"
              >
                {t('stt.addApiKey')}
              </button>
            </div>
          )}

          {keyWarning === 'no-llm-model' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mb-2">
              <i className="ri-download-line text-amber-500" />
              <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('pp.downloadLlmFirst')}</p>
              <button
                onClick={() => { setKeyWarning(null); navigate('/api-keys'); }}
                className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 underline cursor-pointer whitespace-nowrap"
              >
                {t('pp.downloadModels')}
              </button>
            </div>
          )}

          <SettingRow label={t('pp.provider')} description={t('pp.providerDesc')}>
            <Select
              value={config.post_processing.provider}
              onChange={(val) => {
                setKeyWarning(null);
                update((c) => { c.post_processing.provider = val; });
              }}
              options={[
                { value: 'openai', label: t('pp.openai') },
                { value: 'ollama', label: t('pp.ollama') },
              ]}
            />
          </SettingRow>

          {config.post_processing.provider === 'openai' && (
            <SettingRow label={t('stt.model')} description={t('pp.modelOpenaiDesc')}>
              <Select
                value={config.post_processing.model}
                onChange={(val) => update((c) => { c.post_processing.model = val; })}
                options={[
                  { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
                  { value: 'gpt-4o', label: 'gpt-4o' },
                  { value: 'gpt-4.1-nano', label: 'gpt-4.1-nano' },
                  { value: 'gpt-4.1-mini', label: 'gpt-4.1-mini' },
                  { value: 'gpt-4.1', label: 'gpt-4.1' },
                ]}
              />
            </SettingRow>
          )}

          {config.post_processing.provider === 'ollama' && (
            <SettingRow label={t('stt.model')} description={t('pp.modelLocalDesc')}>
              {config.post_processing.ollama_model ? (
                <button
                  type="button"
                  onClick={scrollToOllamaSection}
                  className="inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium text-slate-700 dark:text-slate-200 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-600 transition-colors cursor-pointer"
                >
                  <span>{llmModels.find((m) => m.id === config.post_processing.ollama_model)?.display_name ?? config.post_processing.ollama_model}</span>
                  <i className="ri-arrow-down-line text-base text-amber-500" />
                </button>
              ) : (
                <Select
                  value=""
                  onChange={(val) => {
                    const m = llmModels.find((m) => m.id === val);
                    if (m && !m.downloaded) {
                      setLlmModelPrompt({ id: m.id, displayName: m.display_name, downloading: false });
                      return;
                    }
                    update((c) => {
                      c.post_processing.ollama_model = val;
                      c.post_processing.enabled = true;
                    });
                  }}
                  options={llmModels.map((m) => ({
                    value: m.id,
                    label: m.display_name,
                  }))}
                  placeholder={t('topbar.selectModel')}
                />
              )}
            </SettingRow>
          )}

          <div className="pt-4">
            <div className="flex items-center justify-between mb-3">
              <div>
                <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('pp.systemPrompt')}</p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('pp.systemPromptDesc')}</p>
              </div>
              <div className="flex items-center gap-2">
                <Select
                  value={
                    config.post_processing.prompt === DEFAULT_PP_PROMPT ? '__default__'
                    : (config.post_processing.saved_prompts ?? []).find((p) => p.prompt === config.post_processing.prompt)?.name
                    ?? '__custom__'
                  }
                  onChange={(v) => {
                    if (v === '__custom__') return;
                    if (v === '__default__') { update((c) => { c.post_processing.prompt = DEFAULT_PP_PROMPT; }); return; }
                    const found = config.post_processing.saved_prompts?.find((p) => p.name === v);
                    if (found) update((c) => { c.post_processing.prompt = found.prompt; });
                  }}
                  options={[
                    { value: '__default__', label: `${config.post_processing.default_emoji || '✏️'} ${t('common.default')}` },
                    ...(config.post_processing.saved_prompts ?? []).map((p) => ({ value: p.name, label: `${p.emoji || '📝'} ${p.name}` })),
                    ...(config.post_processing.prompt !== DEFAULT_PP_PROMPT &&
                        !(config.post_processing.saved_prompts ?? []).some((p) => p.prompt === config.post_processing.prompt)
                      ? [{ value: '__custom__', label: t('pp.customUnsaved') }]
                      : []),
                  ]}
                />
              </div>
            </div>

            <textarea
              ref={promptTextareaRef}
              value={config.post_processing.prompt}
              onChange={(e) => updateDebounced((c) => { c.post_processing.prompt = e.target.value; })}
              className="grip-corner text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-4 pt-3.5 pb-8 outline-none w-full min-h-32 resize-y font-mono my-3"
            />

            <div className="flex items-center justify-between">
              <p className="text-slate-400 dark:text-slate-500 text-[11px] leading-relaxed flex items-start gap-1 max-w-[60%]">
                <i className="ri-information-line text-xs mt-0.5 flex-shrink-0" />{t('pp.promptDisclaimer')}
              </p>
              <div className="flex items-center gap-1.5">
                <button
                  onClick={() => setPromptModal(true)}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 border border-amber-200 dark:border-amber-500/30 transition-colors cursor-pointer"
                >
                  <i className="ri-expand-diagonal-line text-sm" />
                  {t('pp.expand')}
                </button>
                <button
                  onClick={() => {
                    const match = (config.post_processing.saved_prompts ?? []).find((p) => p.prompt === config.post_processing.prompt);
                    if (match) {
                      setEditingPrompt(match.name);
                      setEditName(match.name);
                      setEditBody(match.prompt);
                      setEditEmoji(match.emoji || '📝');
                    } else {
                      setEditingPrompt('__default__');
                      setEditName('Default');
                      setEditBody(config.post_processing.prompt);
                      setEditEmoji(config.post_processing.default_emoji || '✏️');
                    }
                  }}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-500/10 hover:bg-blue-100 dark:hover:bg-blue-500/20 border border-blue-200 dark:border-blue-500/30 transition-colors cursor-pointer"
                >
                  <i className="ri-edit-line text-sm" />
                  {t('pp.edit')}
                </button>
                <button
                  onClick={() => {
                    setEditingPrompt('__new__');
                    setEditName('');
                    setEditBody('');
                    setEditEmoji('📝');
                  }}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-500/10 hover:bg-emerald-100 dark:hover:bg-emerald-500/20 border border-emerald-200 dark:border-emerald-500/30 transition-colors cursor-pointer"
                >
                  <i className="ri-add-line text-sm" />
                  {t('pp.new')}
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* Ollama settings (visible only when provider === ollama) */}
        {config.post_processing.provider === 'ollama' && (
          <div ref={ollamaSectionRef} className="scroll-mt-4">
            <OllamaSettings config={config} update={update} />
          </div>
        )}

        {/* Legacy LLM Models grid — retained for non-Ollama providers */}
        {false && (
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('pp.llmModels')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('pp.llmModelsDesc')}</p>

          <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
            {llmModels.map((m) => {
              const isDownloading = llmDownloadProgress?.model === m.id;
              const pct = isDownloading && llmDownloadProgress.total > 0
                ? Math.round((llmDownloadProgress.downloaded / llmDownloadProgress.total) * 100) : 0;
              const size = m.size_bytes >= 1_000_000_000
                ? `${(m.size_bytes / 1_000_000_000).toFixed(1)} GB`
                : `${(m.size_bytes / 1_000_000).toFixed(0)} MB`;
              return (
                <div key={m.id} className="flex items-center justify-between py-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <span className="text-slate-800 dark:text-slate-200 text-sm font-medium">{m.display_name}</span>
                    <span className="text-slate-400 dark:text-slate-500 text-xs">{size}</span>
                    {m.downloaded && <span className="text-emerald-500 text-xs font-medium flex items-center gap-1"><i className="ri-check-line text-xs" />{t('common.downloaded')}</span>}
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
                        >{t('common.cancel')}</button>
                      </>
                    ) : m.downloaded ? (
                      <button
                        onClick={() => api.deleteLlmModel(m.id).then(() => dispatch(fetchLlmModels()))}
                        className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all"
                      >{t('common.delete')}</button>
                    ) : (
                      <button
                        onClick={() => api.downloadLlmModel(m.id)}
                        disabled={llmDownloadProgress !== null}
                        className="px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 border border-amber-200 dark:border-amber-500/30 rounded-lg cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                      >{t('common.download')}</button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
        )}

        {saved && (
          <div className="flex items-center justify-end">
            <span className="text-emerald-500 text-xs font-medium">{t('common.saved')}</span>
          </div>
        )}
      </div>

      {/* Prompt editor modal */}
      {promptModal && config && (
        <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6" onClick={() => setPromptModal(false)}>
          <div
            className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-2xl flex flex-col"
            style={{ maxHeight: 'calc(100vh - 3rem)' }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-6 pt-5 pb-3">
              <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('pp.systemPrompt')}</h3>
              <button
                onClick={() => setPromptModal(false)}
                className="p-1 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors cursor-pointer"
              >
                <i className="ri-close-line text-lg" />
              </button>
            </div>
            <div className="px-6 pb-1 flex-1 min-h-0">
              <textarea
                autoFocus
                value={config.post_processing.prompt}
                onChange={(e) => updateDebounced((c) => { c.post_processing.prompt = e.target.value; })}
                className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-4 py-3 outline-none w-full h-80 resize-y font-mono"
              />
            </div>
            <div className="px-6 pb-5 pt-3 flex flex-col gap-3">
              <div className="flex items-center gap-2 flex-wrap">
                {savingPromptName ? (
                  <form
                    className="inline-flex items-center gap-1.5"
                    onSubmit={(e) => {
                      e.preventDefault();
                      if (!promptNameDraft.trim()) return;
                      update((c) => {
                        if (!c.post_processing.saved_prompts) c.post_processing.saved_prompts = [];
                        const existing = c.post_processing.saved_prompts.findIndex((p) => p.name === promptNameDraft.trim());
                        if (existing >= 0) {
                          c.post_processing.saved_prompts[existing].prompt = c.post_processing.prompt;
                        } else {
                          c.post_processing.saved_prompts.push({ name: promptNameDraft.trim(), prompt: c.post_processing.prompt, emoji: '📝' });
                        }
                      });
                      setSavingPromptName(false);
                      setPromptNameDraft('');
                    }}
                  >
                    <input
                      autoFocus
                      value={promptNameDraft}
                      onChange={(e) => setPromptNameDraft(e.target.value)}
                      onKeyDown={(e) => { if (e.key === 'Escape') { setSavingPromptName(false); setPromptNameDraft(''); } }}
                      placeholder={t('pp.promptName')}
                      className="px-2.5 py-1 rounded-lg text-xs text-slate-700 dark:text-slate-300 bg-white dark:bg-slate-700 border border-blue-300 dark:border-blue-500 outline-none w-36"
                    />
                    <button type="submit" className="px-2 py-1 rounded-lg text-xs font-medium text-white bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer">{t('pp.savePrompt')}</button>
                    <button
                      type="button"
                      onClick={() => { setSavingPromptName(false); setPromptNameDraft(''); }}
                      className="px-1.5 py-1 rounded-lg text-xs text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 cursor-pointer"
                    >
                      <i className="ri-close-line" />
                    </button>
                  </form>
                ) : (
                  <button
                    onClick={() => setSavingPromptName(true)}
                    className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-500/10 hover:bg-blue-100 dark:hover:bg-blue-500/20 border border-blue-200 dark:border-blue-500/30 transition-colors cursor-pointer"
                  >
                    <i className="ri-save-line text-sm" />
                    {t('pp.saveAs')}
                  </button>
                )}
              </div>
              {config.post_processing.saved_prompts?.length > 0 && (
                <div className="flex flex-col gap-1">
                  <span className="text-[10px] font-semibold text-slate-400 dark:text-slate-500 uppercase tracking-widest">{t('pp.savedPrompts')}</span>
                  <div className="flex flex-wrap gap-1.5">
                    {config.post_processing.saved_prompts.map((sp) => (
                      <div key={sp.name} className="inline-flex items-center rounded-lg border border-slate-200 dark:border-slate-600 overflow-hidden">
                        <button
                          onClick={() => update((c) => { c.post_processing.prompt = sp.prompt; })}
                          className="px-2.5 py-1 text-xs font-medium text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors cursor-pointer"
                          title={sp.prompt.slice(0, 100)}
                        >
                          {sp.name}
                        </button>
                        <button
                          onClick={() => update((c) => {
                            c.post_processing.saved_prompts = c.post_processing.saved_prompts.filter((p) => p.name !== sp.name);
                          })}
                          className="px-1.5 py-1 text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors cursor-pointer border-l border-slate-200 dark:border-slate-600"
                          title={t('common.delete')}
                        >
                          <i className="ri-close-line text-xs" />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Edit / New prompt modal */}
      {editingPrompt !== null && (() => {
        const isDefault = editingPrompt === '__default__';
        const isNew = editingPrompt === '__new__';
        const seedMatch = SEED_PROMPTS.find((s) => s.name === editingPrompt);
        const isBuiltIn = isDefault || !!seedMatch;
        return (
          <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6" onClick={() => { setConfirmDelete(false); setEditingPrompt(null); }}>
            <div
              className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-2xl flex flex-col"
              style={{ maxHeight: 'calc(100vh - 3rem)' }}
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between px-6 pt-5 pb-3">
                <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">
                  {isNew ? t('pp.createPrompt') : isDefault ? t('pp.editPrompt') : `${t('pp.edit')}: ${editingPrompt}`}
                </h3>
                <button
                  onClick={() => { setConfirmDelete(false); setEditingPrompt(null); }}
                  className="p-1 rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors cursor-pointer"
                >
                  <i className="ri-close-line text-lg" />
                </button>
              </div>
              <div className="px-6 flex flex-col gap-3 flex-1 min-h-0">
                <div className="flex gap-3">
                  <div className="flex-1">
                    <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-1 block">{t('pp.name')}</label>
                    <input
                      autoFocus={isNew}
                      value={editName}
                      onChange={(e) => !isDefault && setEditName(e.target.value)}
                      readOnly={isDefault}
                      placeholder="e.g. Formal, Casual, Bullet Points..."
                      className={`w-full text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none ${isDefault ? 'opacity-50 cursor-not-allowed' : ''}`}
                    />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-1 block">{t('pp.icon')}</label>
                    <EmojiPicker value={editEmoji} onChange={setEditEmoji} />
                  </div>
                </div>
                <div className="flex-1 min-h-0">
                  <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-1 block">{t('pp.prompt')}</label>
                  <textarea
                    autoFocus={!isNew}
                    value={editBody}
                    onChange={(e) => setEditBody(e.target.value)}
                    placeholder="Enter the system prompt instructions..."
                    className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-4 py-3 outline-none w-full h-64 resize-y font-mono"
                  />
                </div>
              </div>
              <div className="px-6 pb-5 pt-4 flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {isBuiltIn && (
                    <button
                      onClick={() => {
                        if (seedMatch) {
                          setEditBody(seedMatch.prompt);
                          setEditEmoji(seedMatch.emoji);
                        } else {
                          setEditBody(DEFAULT_PP_PROMPT);
                          setEditEmoji('✏️');
                        }
                      }}
                      className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-500 dark:text-slate-400 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600 cursor-pointer transition-all"
                    >
                      <i className="ri-restart-line text-sm mr-1" />{t('pp.resetToDefault')}
                    </button>
                  )}
                  {!isDefault && !isNew && !confirmDelete && (
                    <button
                      onClick={() => setConfirmDelete(true)}
                      className="px-3 py-1.5 rounded-lg text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 cursor-pointer transition-all"
                    >
                      <i className="ri-delete-bin-line text-sm mr-1" />{t('pp.deletePrompt')}
                    </button>
                  )}
                  {!isDefault && !isNew && confirmDelete && (
                    <div className="flex items-center gap-2">
                      <span className="text-xs text-slate-500 dark:text-slate-400">{t('pp.confirmDelete')}</span>
                      <button
                        onClick={() => {
                          update((c) => {
                            c.post_processing.saved_prompts = (c.post_processing.saved_prompts ?? []).filter((p) => p.name !== editingPrompt);
                            if (c.post_processing.prompt === editBody) {
                              c.post_processing.prompt = DEFAULT_PP_PROMPT;
                            }
                          });
                          setConfirmDelete(false);
                          setEditingPrompt(null);
                        }}
                        className="px-2.5 py-1 rounded-lg text-xs font-medium text-white bg-red-500 hover:bg-red-600 cursor-pointer transition-all"
                      >
                        {t('common.yes')}
                      </button>
                      <button
                        onClick={() => setConfirmDelete(false)}
                        className="px-2.5 py-1 rounded-lg text-xs font-medium text-slate-600 dark:text-slate-400 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600 cursor-pointer transition-all"
                      >
                        {t('common.no')}
                      </button>
                    </div>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => setEditingPrompt(null)}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-600 dark:text-slate-400 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600 cursor-pointer transition-all"
                  >
                    {t('common.cancel')}
                  </button>
                  <button
                    onClick={() => {
                      if (!editBody.trim()) return;
                      if (!isDefault && !editName.trim()) return;
                      update((c) => {
                        if (isDefault) {
                          c.post_processing.prompt = editBody;
                          c.post_processing.default_emoji = editEmoji;
                        } else {
                          if (!c.post_processing.saved_prompts) c.post_processing.saved_prompts = [];
                          if (isNew) {
                            const existing = c.post_processing.saved_prompts.findIndex((p) => p.name === editName.trim());
                            if (existing >= 0) {
                              c.post_processing.saved_prompts[existing].prompt = editBody;
                              c.post_processing.saved_prompts[existing].emoji = editEmoji;
                            } else {
                              c.post_processing.saved_prompts.push({ name: editName.trim(), prompt: editBody, emoji: editEmoji });
                            }
                          } else {
                            const idx = c.post_processing.saved_prompts.findIndex((p) => p.name === editingPrompt);
                            if (idx >= 0) {
                              c.post_processing.saved_prompts[idx].name = editName.trim();
                              c.post_processing.saved_prompts[idx].prompt = editBody;
                              c.post_processing.saved_prompts[idx].emoji = editEmoji;
                            }
                          }
                          c.post_processing.prompt = editBody;
                        }
                      });
                      setEditingPrompt(null);
                    }}
                    disabled={!editBody.trim() || (!isDefault && !editName.trim())}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-amber-500 hover:bg-amber-600 cursor-pointer transition-all disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    {isNew ? t('pp.create') : t('pp.savePrompt')}
                  </button>
                </div>
              </div>
            </div>
          </div>
        );
      })()}

      {/* LLM model download modal */}
      {llmModelPrompt && (
        <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6">
          <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-sm" onClick={(e) => e.stopPropagation()}>
            <div className="p-6 text-center">
              <div className="w-10 h-10 rounded-full bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center mx-auto mb-3">
                <i className="ri-download-line text-amber-500 text-xl" />
              </div>
              <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.modelNotDownloaded')}</h3>
              <p className="text-slate-500 dark:text-slate-400 text-xs">
                {t('stt.modelNotDownloadedDesc', { model: llmModelPrompt.displayName })}
              </p>
              {llmModelPrompt.downloading && llmDownloadProgress && (
                <div className="mt-4 flex items-center gap-3">
                  <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-2 overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-all ${llmDownloadProgress.verifying ? 'bg-sky-400 animate-pulse' : 'bg-amber-400'}`}
                      style={{ width: llmDownloadProgress.verifying ? '100%' : `${llmDownloadProgress.total > 0 ? Math.round((llmDownloadProgress.downloaded / llmDownloadProgress.total) * 100) : 0}%` }}
                    />
                  </div>
                  <span className={`text-xs tabular-nums whitespace-nowrap ${llmDownloadProgress.verifying ? 'text-sky-500' : 'text-slate-500 dark:text-slate-400 w-10 text-right'}`}>
                    {llmDownloadProgress.verifying ? t('common.verifying') : `${llmDownloadProgress.total > 0 ? Math.round((llmDownloadProgress.downloaded / llmDownloadProgress.total) * 100) : 0}%`}
                  </span>
                </div>
              )}
            </div>
            <div className="px-6 pb-5 flex justify-center gap-3">
              {llmModelPrompt.downloading ? (
                <button onClick={() => api.cancelLlmModelDownload()} className="px-4 py-2 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all">{t('common.cancel')}</button>
              ) : (
                <>
                  <button onClick={() => setLlmModelPrompt(null)} className="px-4 py-2 text-xs font-medium text-slate-600 dark:text-slate-400 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600 rounded-lg cursor-pointer transition-all">{t('common.cancel')}</button>
                  <button
                    onClick={() => {
                      setLlmModelPrompt({ ...llmModelPrompt, downloading: true });
                      update((c) => { c.post_processing.ollama_model = llmModelPrompt.id; });
                      api.downloadLlmModel(llmModelPrompt.id);
                    }}
                    className="px-4 py-2 text-xs font-medium text-white bg-amber-500 hover:bg-amber-600 rounded-lg cursor-pointer transition-all"
                  >{t('common.download')}</button>
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </Layout>
  );
}

// ── Ollama settings panel ──────────────────────────────────────────────

type StepKey = 'download' | 'extract' | 'spawn' | 'health';
type StepStatus = 'pending' | 'running' | 'ok' | 'error';
type InstallState = {
  open: boolean;
  steps: Record<StepKey, StepStatus>;
  currentStep: StepKey | null;
  downloaded: number;
  total: number | null;
  message: string | null;
  logs: string[];
  done: boolean;
  error: string | null;
};

const INITIAL_INSTALL: InstallState = {
  open: false,
  steps: { download: 'pending', extract: 'pending', spawn: 'pending', health: 'pending' },
  currentStep: null,
  downloaded: 0,
  total: null,
  message: null,
  logs: [],
  done: false,
  error: null,
};

type ConfirmRequest = {
  title: string;
  message: string;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
};

type UninstallStep = 'stop' | 'remove';
type UninstallStatus = 'pending' | 'running' | 'ok' | 'error';
type UninstallState = {
  open: boolean;
  steps: Record<UninstallStep, UninstallStatus>;
  message: string | null;
  done: boolean;
  error: string | null;
};

const INITIAL_UNINSTALL: UninstallState = {
  open: false,
  steps: { stop: 'pending', remove: 'pending' },
  message: null,
  done: false,
  error: null,
};

type UninstallEventPayload = {
  step: 'stop' | 'remove' | 'done';
  status: 'start' | 'ok' | 'error';
  message: string | null;
  done: boolean;
  error: string | null;
};

type PullState = {
  running: boolean;
  model: string | null;
  status: string;       // last "status" field from Ollama (e.g., "pulling manifest", "downloading", "success")
  completed: number;    // bytes downloaded for current layer
  total: number;        // bytes total for current layer
  digest: string | null;
  error: string | null;
  ready: boolean;       // true on success
};

const INITIAL_PULL: PullState = {
  running: false, model: null, status: '', completed: 0, total: 0,
  digest: null, error: null, ready: false,
};

type InstallEventPayload = {
  step: 'download' | 'extract' | 'spawn' | 'health' | 'log' | 'done';
  status: 'start' | 'progress' | 'ok' | 'error';
  downloaded: number;
  total: number | null;
  message: string | null;
  done: boolean;
  error: string | null;
  logs: string[] | null;
};

function OllamaSettings({ config, update }: { config: Config; update: (fn: (c: Config) => void) => void }) {
  const { t } = useTranslation();
  const pp = config.post_processing;
  const [detect, setDetect] = useState<{ reachable: boolean; version: string | null; models: string[] } | null>(null);
  const [install, setInstall] = useState<InstallState>(INITIAL_INSTALL);
  const [pull, setPull] = useState<PullState>(INITIAL_PULL);
  const [browserOpen, setBrowserOpen] = useState(false);
  const [systemInfo, setSystemInfo] = useState<import('@/lib/types').SystemInfo | null>(null);
  const [managedInstalled, setManagedInstalled] = useState<boolean | null>(null);
  const [uninstall, setUninstall] = useState<UninstallState>(INITIAL_UNINSTALL);
  const [pendingConfirm, setPendingConfirm] = useState<ConfirmRequest | null>(null);

  const refreshManaged = async () => {
    try { setManagedInstalled(await api.ollamaManagedInstalled()); }
    catch { setManagedInstalled(null); }
  };

  useEffect(() => {
    api.getSystemInfo().then(setSystemInfo).catch(() => setSystemInfo(null));
    refreshManaged();
  }, []);

  // Refresh installed status when install or uninstall finishes.
  useEffect(() => { if (install.done && !install.error) refreshManaged(); }, [install.done, install.error]);
  useEffect(() => { if (uninstall.done) refreshManaged(); }, [uninstall.done]);

  // Auto-start the managed daemon when Ollama is the active backend but
  // nothing is currently listening — covers the "settings got reset, daemon
  // didn't auto-spawn at launch" case. Idempotent on the backend.
  const [restarting, setRestarting] = useState(false);
  useEffect(() => {
    if (pp.provider !== 'ollama') return;
    if (pp.ollama_mode !== 'managed') return;
    if (managedInstalled !== true) return;
    if (detect === null) return; // first probe hasn't returned yet
    if (detect.reachable) return;
    api.ollamaStart().then((started) => {
      if (started) setTimeout(refresh, 1500);
    }).catch((e) => { console.warn('ollama auto-start failed', e); });
  }, [pp.provider, pp.ollama_mode, managedInstalled, detect?.reachable]);

  const onRestart = async () => {
    setRestarting(true);
    try {
      await api.ollamaRestart();
      setTimeout(refresh, 1500);
    } catch (e) {
      console.error('ollama restart failed', e);
    } finally {
      setRestarting(false);
    }
  };

  const startPull = async (tag: string) => {
    setPull({ ...INITIAL_PULL, running: true, model: tag, status: t('pp.ollamaPullStarting') });
    try { await api.ollamaPullModel(tag); }
    catch (e) { setPull((p) => ({ ...p, running: false, error: String(e), ready: false })); }
  };

  const refresh = async () => {
    try { setDetect(await api.ollamaDetect()); } catch { setDetect({ reachable: false, version: null, models: [] }); }
  };

  useEffect(() => { refresh(); }, [pp.ollama_mode, pp.ollama_url, pp.ollama_bundled_port]);

  useEffect(() => {
    const off1 = listenTauriEvent<InstallEventPayload>('ollama-install-progress', (ev) => {
      const p = ev.payload;
      setInstall((prev) => {
        // Only react to events while a user-initiated install is open.
        // Startup auto-spawn events still arrive but should not pop the modal.
        if (!prev.open && !p.done) return prev;

        const next: InstallState = { ...prev, steps: { ...prev.steps } };
        if (p.message) next.logs = [...prev.logs, p.message];

        if (p.done) {
          next.done = true;
          next.error = p.error;
          if (p.error) {
            if (prev.currentStep) next.steps[prev.currentStep] = 'error';
            if (p.logs && p.logs.length > 0) next.logs = p.logs;
          } else {
            next.steps = { download: 'ok', extract: 'ok', spawn: 'ok', health: 'ok' };
            next.currentStep = null;
            next.message = null;
          }
          refresh();
          return next;
        }

        if (p.step === 'log') {
          next.message = p.message;
          return next;
        }

        const stepKey = p.step as StepKey;
        if (p.status === 'start' || p.status === 'progress') {
          // Mark prior step ok if we advanced.
          const order: StepKey[] = ['download', 'extract', 'spawn', 'health'];
          const idx = order.indexOf(stepKey);
          for (let i = 0; i < idx; i++) {
            if (next.steps[order[i]] !== 'ok' && next.steps[order[i]] !== 'error') {
              next.steps[order[i]] = 'ok';
            }
          }
          next.steps[stepKey] = 'running';
          next.currentStep = stepKey;
          if (p.step === 'download' && p.status === 'progress') {
            next.downloaded = p.downloaded;
            next.total = p.total;
          }
          if (p.message) next.message = p.message;
        } else if (p.status === 'ok') {
          next.steps[stepKey] = 'ok';
        } else if (p.status === 'error') {
          next.steps[stepKey] = 'error';
        }
        return next;
      });
    });
    const off2 = listenTauriEvent<{ model: string; line: string; done: boolean; error: string | null }>('ollama-pull-progress', (ev) => {
      const p = ev.payload;
      setPull((prev) => {
        // Stream finished.
        if (p.done) {
          if (p.error) {
            return { ...prev, running: false, error: p.error, ready: false };
          }
          refresh();
          return { ...prev, running: false, error: null, ready: true, status: 'success', completed: prev.total };
        }
        // Mid-stream JSON line from Ollama. Each line is a JSON object like:
        //   {"status":"pulling manifest"}
        //   {"status":"downloading","digest":"sha256:abc","total":N,"completed":M}
        //   {"status":"verifying sha256 digest"}
        //   {"status":"success"}
        let parsed: { status?: string; digest?: string; total?: number; completed?: number; error?: string } = {};
        try { parsed = JSON.parse(p.line); } catch { /* ignore non-JSON lines */ }
        if (parsed.error) {
          return { ...prev, running: false, error: parsed.error, ready: false };
        }
        const status = parsed.status ?? prev.status;
        const isDownloadStep = status === 'downloading' && typeof parsed.total === 'number';
        const isNewLayer = isDownloadStep && parsed.digest && parsed.digest !== prev.digest;
        return {
          ...prev,
          running: true,
          model: p.model || prev.model,
          status,
          digest: parsed.digest ?? prev.digest,
          // When a new layer starts, reset bytes to that layer's totals.
          completed: isDownloadStep ? (parsed.completed ?? 0) : (isNewLayer ? 0 : prev.completed),
          total: isDownloadStep ? (parsed.total ?? prev.total) : prev.total,
          error: null,
          ready: false,
        };
      });
    });
    const off3 = listenTauriEvent<UninstallEventPayload>('ollama-uninstall-progress', (ev) => {
      const p = ev.payload;
      setUninstall((prev) => {
        if (!prev.open && !p.done) return prev;
        const next: UninstallState = { ...prev, steps: { ...prev.steps } };
        if (p.message) next.message = p.message;
        if (p.done) {
          next.done = true;
          next.error = p.error;
          if (!p.error) {
            next.steps = { stop: 'ok', remove: 'ok' };
          } else {
            // Mark whichever step was running as errored.
            const order: UninstallStep[] = ['stop', 'remove'];
            for (const s of order) {
              if (next.steps[s] === 'running') { next.steps[s] = 'error'; break; }
            }
          }
          return next;
        }
        if (p.step === 'stop' || p.step === 'remove') {
          if (p.status === 'start') next.steps[p.step] = 'running';
          else if (p.status === 'ok') next.steps[p.step] = 'ok';
          else if (p.status === 'error') next.steps[p.step] = 'error';
        }
        return next;
      });
    });
    return () => { off1.then((f) => f()); off2.then((f) => f()); off3.then((f) => f()); };
  }, []);

  const startInstall = async () => {
    setInstall({ ...INITIAL_INSTALL, open: true, steps: { ...INITIAL_INSTALL.steps, download: 'running' }, currentStep: 'download' });
    try {
      await api.ollamaInstall();
    } catch (e) {
      setInstall((prev) => ({ ...prev, done: true, error: String(e) }));
    }
  };

  const closeInstall = () => setInstall(INITIAL_INSTALL);

  const startUninstall = async () => {
    setPendingConfirm({
      title: t('pp.ollamaUninstallTitle'),
      message: t('pp.ollamaUninstallConfirm'),
      confirmLabel: t('pp.ollamaUninstall'),
      destructive: true,
      onConfirm: async () => {
        setUninstall({ ...INITIAL_UNINSTALL, open: true, steps: { stop: 'running', remove: 'pending' } });
        try { await api.ollamaUninstall(); }
        catch (e) { setUninstall((p) => ({ ...p, done: true, error: String(e) })); }
      },
    });
  };
  const closeUninstall = () => setUninstall(INITIAL_UNINSTALL);

  const installing = install.open && !install.done;
  const uninstalling = uninstall.open && !uninstall.done;

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5 flex flex-col gap-4">
      <div>
        <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">Ollama</h3>
        <p className="text-slate-400 dark:text-slate-500 text-xs">{t('pp.providerDesc')}</p>
      </div>

      <div
        role="radiogroup"
        aria-label="Ollama mode"
        className="grid grid-cols-1 sm:grid-cols-3 gap-2"
      >
        {[
          { value: 'managed', label: t('pp.ollamaModeManaged'), desc: t('pp.ollamaModeManagedDesc') },
          { value: 'existing', label: t('pp.ollamaModeExisting'), desc: t('pp.ollamaModeExistingDesc') },
          { value: 'custom', label: t('pp.ollamaModeCustom'), desc: t('pp.ollamaModeCustomDesc') },
        ].map((opt) => {
          const selected = pp.ollama_mode === opt.value;
          return (
            <button
              key={opt.value}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => update((c) => { c.post_processing.ollama_mode = opt.value; })}
              className={`text-left px-3 py-2.5 rounded-lg border transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-amber-500/60 ${
                selected
                  ? 'border-amber-500 bg-amber-50 dark:bg-amber-500/10 ring-1 ring-amber-500/40'
                  : 'border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-white dark:hover:bg-slate-700/40'
              }`}
            >
              <div className={`text-xs font-semibold ${
                selected
                  ? 'text-amber-700 dark:text-amber-300'
                  : 'text-slate-800 dark:text-slate-200'
              }`}>{opt.label}</div>
              <div className="text-[11px] mt-0.5 leading-snug text-slate-500 dark:text-slate-400">{opt.desc}</div>
            </button>
          );
        })}
      </div>

      {pp.ollama_mode === 'managed' && (() => {
        // Treat "installed" as: backend confirmed, OR the managed daemon is
        // currently reachable. The latter handles the case where a previous
        // build of Verbatim installed the binary but the new
        // `ollama_managed_installed` IPC isn't available yet (state == null).
        const looksInstalled = managedInstalled === true || (managedInstalled !== false && detect?.reachable === true);
        return (
          <div className="flex items-center gap-3 flex-wrap">
            {looksInstalled ? (
              <>
                <span className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-500/10 border border-emerald-200 dark:border-emerald-500/30">
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2.5" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7"/></svg>
                  {t('pp.ollamaInstalled')}
                </span>
                <button
                  onClick={onRestart}
                  disabled={restarting || uninstalling}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-sky-700 dark:text-sky-300 bg-sky-50 dark:bg-sky-500/10 border border-sky-200 dark:border-sky-500/30 hover:bg-sky-100 dark:hover:bg-sky-500/20 disabled:opacity-50"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M4 4v6h6M20 20v-6h-6M5 14a7 7 0 0011.6 3M19 10A7 7 0 007.4 7"/></svg>
                  {restarting ? t('pp.ollamaRestarting') : t('pp.ollamaRestart')}
                </button>
                <button
                  onClick={startUninstall}
                  disabled={uninstalling || restarting}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-rose-700 dark:text-rose-300 bg-rose-50 dark:bg-rose-500/10 border border-rose-200 dark:border-rose-500/30 hover:bg-rose-100 dark:hover:bg-rose-500/20 disabled:opacity-50"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3"/></svg>
                  {uninstalling ? t('pp.ollamaUninstallProgress') : t('pp.ollamaUninstall')}
                </button>
              </>
            ) : (
              <button
                onClick={startInstall}
                disabled={installing}
                className="px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-amber-500 hover:bg-amber-600 disabled:opacity-50"
              >{installing ? t('pp.ollamaInstallProgress') : t('pp.ollamaInstall')}</button>
            )}
            <label className="text-xs text-slate-600 dark:text-slate-400 flex items-center gap-2">
              {t('pp.ollamaPort')}
              <input
                type="number"
                min={1024}
                max={65535}
                value={pp.ollama_bundled_port}
                onChange={(e) => update((c) => { c.post_processing.ollama_bundled_port = parseInt(e.target.value) || 11434; })}
                className="w-24 px-2 py-1 rounded border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 text-slate-700 dark:text-slate-300"
              />
            </label>
          </div>
        );
      })()}

      <OllamaInstallModal state={install} onClose={closeInstall} />
      <OllamaUninstallModal state={uninstall} onClose={closeUninstall} />

      {pp.ollama_mode === 'custom' && (
        <div className="flex flex-col gap-2">
          <label className="text-xs text-slate-600 dark:text-slate-400">{t('pp.ollamaUrl')}
            <input
              value={pp.ollama_url}
              onChange={(e) => update((c) => { c.post_processing.ollama_url = e.target.value; })}
              className="mt-1 w-full px-3 py-1.5 rounded border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 text-slate-700 dark:text-slate-300 text-sm"
            />
          </label>
          <label className="text-xs text-slate-600 dark:text-slate-400">{t('pp.ollamaAuthToken')}
            <input
              type="password"
              value={pp.ollama_auth_token}
              onChange={(e) => update((c) => { c.post_processing.ollama_auth_token = e.target.value; })}
              className="mt-1 w-full px-3 py-1.5 rounded border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 text-slate-700 dark:text-slate-300 text-sm"
            />
          </label>
        </div>
      )}

      <div className="flex items-center gap-2 text-xs">
        <button onClick={refresh} className="px-2.5 py-1 rounded border border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600">{t('pp.ollamaTestConnection')}</button>
        {detect?.reachable ? (
          <span className="text-emerald-500">{t('pp.ollamaRunning')}{detect.version ? ` · ${detect.version}` : ''}</span>
        ) : detect ? (
          <span className="text-amber-500">{t('pp.ollamaNotFound')}</span>
        ) : null}
      </div>

      <div className="flex flex-col gap-2">
        <div className="flex flex-col gap-1">
          <span className="text-xs text-slate-600 dark:text-slate-400">{t('pp.ollamaModel')}</span>
          <Select
            value={(detect?.models.length ?? 0) === 0 ? '' : pp.ollama_model}
            onChange={(val) => update((c) => { c.post_processing.ollama_model = val; })}
            options={(detect?.models ?? []).map((m) => ({ value: m, label: m }))}
            placeholder={t('pp.ollamaNoModels')}
          />
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setBrowserOpen(true)}
            disabled={pull.running}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 hover:bg-amber-100 dark:hover:bg-amber-500/20 disabled:opacity-50 inline-flex items-center gap-1.5"
          >
            <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-4.35-4.35M11 19a8 8 0 100-16 8 8 0 000 16z"/></svg>
            {t('pp.ollamaBrowseModels')}
          </button>
          {pull.running && <span className="text-xs text-slate-500 dark:text-slate-400">{t('pp.ollamaPullProgress')}</span>}
        </div>
        <PullProgress state={pull} onDismiss={() => setPull(INITIAL_PULL)} />
      </div>

      <ModelBrowserModal
        open={browserOpen}
        onClose={() => setBrowserOpen(false)}
        systemInfo={systemInfo}
        sttBackend={config.general.backend}
        sttModel={config.whisper.model}
        installedModels={detect?.models ?? []}
        pullState={pull}
        onPull={(tag) => { setBrowserOpen(false); startPull(tag); }}
        onDelete={(tag) => {
          setPendingConfirm({
            title: t('pp.ollamaDeleteModel'),
            message: t('pp.ollamaDeleteModelConfirm', { tag }),
            confirmLabel: t('pp.ollamaDeleteModel'),
            destructive: true,
            onConfirm: async () => {
              try { await api.ollamaDeleteModel(tag); await refresh(); }
              catch (e) { setPendingConfirm({
                title: t('pp.ollamaDeleteModel'),
                message: String(e),
                confirmLabel: t('pp.ollamaClose'),
                onConfirm: () => {},
              }); }
            },
          });
        }}
      />

      <ConfirmModal request={pendingConfirm} onClose={() => setPendingConfirm(null)} />
    </div>
  );
}

// ── Ollama install modal ───────────────────────────────────────────────

function OllamaInstallModal({ state, onClose }: { state: InstallState; onClose: () => void }) {
  const { t } = useTranslation();
  const [showLogs, setShowLogs] = useState(false);
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [state.logs.length, showLogs]);

  if (!state.open) return null;

  const steps: { key: StepKey; label: string }[] = [
    { key: 'download', label: t('pp.ollamaStepDownload') },
    { key: 'extract',  label: t('pp.ollamaStepExtract') },
    { key: 'spawn',    label: t('pp.ollamaStepSpawn') },
    { key: 'health',   label: t('pp.ollamaStepHealth') },
  ];

  const downloadPct = state.total && state.total > 0
    ? Math.min(100, Math.floor((state.downloaded / state.total) * 100))
    : null;

  const fmtBytes = (n: number) => {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  };

  const copyLogs = async () => {
    try {
      await navigator.clipboard.writeText(state.logs.join('\n'));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* ignore */ }
  };

  const closeable = state.done;
  const succeeded = state.done && !state.error;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-md mx-4 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 shadow-xl flex flex-col gap-4 p-6">
        <div className="flex items-start justify-between">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-base">
            {succeeded ? t('pp.ollamaInstallSuccess')
              : state.error ? t('pp.ollamaInstallFailed')
              : t('pp.ollamaInstallTitle')}
          </h3>
        </div>

        <ol className="flex flex-col gap-2">
          {steps.map((s) => {
            const status = state.steps[s.key];
            return (
              <li key={s.key} className="flex items-center gap-2 text-sm">
                <StepIcon status={status} />
                <span className={
                  status === 'error' ? 'text-rose-600 dark:text-rose-400'
                  : status === 'ok' ? 'text-emerald-600 dark:text-emerald-400'
                  : status === 'running' ? 'text-slate-900 dark:text-slate-100'
                  : 'text-slate-400 dark:text-slate-500'
                }>{s.label}</span>
                {s.key === 'download' && status === 'running' && downloadPct !== null && (
                  <span className="text-xs text-slate-500 dark:text-slate-400 ml-auto">
                    {fmtBytes(state.downloaded)}{state.total ? ` / ${fmtBytes(state.total)}` : ''} · {downloadPct}%
                  </span>
                )}
              </li>
            );
          })}
        </ol>

        {state.steps.download === 'running' && downloadPct !== null && (
          <div className="h-1.5 w-full bg-slate-100 dark:bg-slate-700 rounded overflow-hidden">
            <div className="h-full bg-amber-500 transition-all" style={{ width: `${downloadPct}%` }} />
          </div>
        )}

        {state.message && !state.error && (
          <div className="text-xs text-slate-500 dark:text-slate-400 truncate font-mono">{state.message}</div>
        )}

        {state.error && (
          <div className="flex flex-col gap-2">
            <div className="text-xs text-rose-600 dark:text-rose-400 break-words">{state.error}</div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => setShowLogs((v) => !v)}
                className="px-2 py-1 rounded text-xs font-medium text-slate-600 dark:text-slate-300 bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600"
              >{showLogs ? '▼' : '▶'} {t('pp.ollamaViewLogs')}</button>
              {showLogs && (
                <button
                  onClick={copyLogs}
                  className="px-2 py-1 rounded text-xs font-medium text-slate-600 dark:text-slate-300 bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600"
                >{copied ? t('pp.ollamaCopied') : t('pp.ollamaCopyLogs')}</button>
              )}
            </div>
            {showLogs && (
              <div
                ref={logRef}
                className="text-[11px] font-mono text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded p-2 max-h-48 overflow-auto whitespace-pre-wrap"
              >
                {state.logs.length === 0 ? '(no logs)' : state.logs.join('\n')}
              </div>
            )}
          </div>
        )}

        <div className="flex justify-end">
          <button
            onClick={onClose}
            disabled={!closeable}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-slate-600 hover:bg-slate-700 disabled:opacity-50"
          >{t('pp.ollamaClose')}</button>
        </div>
      </div>
    </div>
  );
}

// ── Ollama model-pull progress block ──────────────────────────────────

function PullProgress({ state, onDismiss }: { state: PullState; onDismiss: () => void }) {
  const { t } = useTranslation();
  if (!state.running && !state.error && !state.ready) return null;

  const fmtBytes = (n: number) => {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  };

  const pct = state.total > 0 ? Math.min(100, Math.floor((state.completed / state.total) * 100)) : null;
  const isDownloading = state.status === 'downloading' && pct !== null;

  // Map raw Ollama status strings to a translated label.
  const statusLabel = (() => {
    const s = state.status.toLowerCase();
    if (s.startsWith('pulling manifest')) return t('pp.ollamaPullStatusManifest');
    if (s === 'downloading') return t('pp.ollamaPullStatusDownloading');
    if (s.startsWith('verifying')) return t('pp.ollamaPullStatusVerifying');
    if (s.startsWith('writing manifest')) return t('pp.ollamaPullStatusWritingManifest');
    if (s.startsWith('removing')) return t('pp.ollamaPullStatusCleanup');
    if (s === 'success') return t('pp.ollamaPullStatusSuccess');
    return state.status || t('pp.ollamaPullStarting');
  })();

  return (
    <div className="flex flex-col gap-1.5 mt-1 p-2.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/50">
      <div className="flex items-center gap-2 text-xs">
        {state.error ? (
          <span className="text-rose-500">✕</span>
        ) : state.ready ? (
          <span className="text-emerald-500">✓</span>
        ) : (
          <span className="inline-block w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
        )}
        <span className={
          state.error ? 'text-rose-600 dark:text-rose-400 font-medium'
          : state.ready ? 'text-emerald-600 dark:text-emerald-400 font-medium'
          : 'text-slate-700 dark:text-slate-200 font-medium'
        }>
          {state.error ? t('pp.ollamaPullFailed')
            : state.ready ? t('pp.ollamaPullReady')
            : statusLabel}
        </span>
        {state.model && <span className="text-slate-400 dark:text-slate-500 font-mono">{state.model}</span>}
        {isDownloading && (
          <span className="ml-auto text-slate-500 dark:text-slate-400 text-[11px] font-mono">
            {fmtBytes(state.completed)} / {fmtBytes(state.total)} · {pct}%
          </span>
        )}
        {(state.ready || state.error) && (
          <button
            onClick={onDismiss}
            className="ml-auto text-slate-400 hover:text-slate-600 dark:text-slate-500 dark:hover:text-slate-300 text-[11px]"
          >{t('pp.ollamaClose')}</button>
        )}
      </div>
      {isDownloading && (
        <div className="h-1.5 w-full bg-slate-200 dark:bg-slate-700 rounded overflow-hidden">
          <div className="h-full bg-amber-500 transition-all" style={{ width: `${pct}%` }} />
        </div>
      )}
      {state.error && (
        <div className="text-[11px] text-rose-600 dark:text-rose-400 break-words">{state.error}</div>
      )}
    </div>
  );
}

function StepIcon({ status }: { status: StepStatus }) {
  if (status === 'ok') return <span className="text-emerald-500">✓</span>;
  if (status === 'error') return <span className="text-rose-500">✕</span>;
  if (status === 'running') return (
    <span className="inline-block w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
  );
  return <span className="inline-block w-3 h-3 rounded-full bg-slate-300 dark:bg-slate-600" />;
}

// ── In-app confirm modal (replaces window.confirm) ────────────────────

function ConfirmModal({ request, onClose }: { request: ConfirmRequest | null; onClose: () => void }) {
  const { t } = useTranslation();

  // Close on Escape, confirm on Enter.
  useEffect(() => {
    if (!request) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); onClose(); }
      else if (e.key === 'Enter') { e.preventDefault(); request.onConfirm(); onClose(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [request, onClose]);

  if (!request) return null;

  const handleConfirm = () => {
    request.onConfirm();
    onClose();
  };

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="w-full max-w-sm mx-4 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 shadow-xl flex flex-col gap-4 p-6"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="flex items-start gap-3">
          {request.destructive && (
            <div className="shrink-0 w-9 h-9 rounded-full bg-rose-50 dark:bg-rose-500/10 border border-rose-200 dark:border-rose-500/30 flex items-center justify-center">
              <svg className="w-5 h-5 text-rose-600 dark:text-rose-400" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
              </svg>
            </div>
          )}
          <div className="flex-1 min-w-0">
            <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">{request.title}</h3>
            <p className="mt-1 text-xs text-slate-600 dark:text-slate-300 break-words">{request.message}</p>
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-700 dark:text-slate-200 bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600"
          >{t('common.cancel')}</button>
          <button
            onClick={handleConfirm}
            autoFocus
            className={
              'px-3 py-1.5 rounded-lg text-xs font-medium text-white ' +
              (request.destructive
                ? 'bg-rose-600 hover:bg-rose-700'
                : 'bg-amber-500 hover:bg-amber-600')
            }
          >{request.confirmLabel}</button>
        </div>
      </div>
    </div>
  );
}

// ── Ollama uninstall modal ────────────────────────────────────────────

function OllamaUninstallModal({ state, onClose }: { state: UninstallState; onClose: () => void }) {
  const { t } = useTranslation();
  if (!state.open) return null;

  const steps: { key: UninstallStep; label: string }[] = [
    { key: 'stop',   label: t('pp.ollamaUninstallStepStop') },
    { key: 'remove', label: t('pp.ollamaUninstallStepRemove') },
  ];

  const closeable = state.done;
  const succeeded = state.done && !state.error;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-md mx-4 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 shadow-xl flex flex-col gap-4 p-6">
        <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-base">
          {succeeded ? t('pp.ollamaUninstallSuccess')
            : state.error ? t('pp.ollamaUninstallFailed')
            : t('pp.ollamaUninstallTitle')}
        </h3>

        <ol className="flex flex-col gap-2">
          {steps.map((s) => {
            const status = state.steps[s.key];
            return (
              <li key={s.key} className="flex items-center gap-2 text-sm">
                <StepIcon status={status} />
                <span className={
                  status === 'error' ? 'text-rose-600 dark:text-rose-400'
                  : status === 'ok' ? 'text-emerald-600 dark:text-emerald-400'
                  : status === 'running' ? 'text-slate-900 dark:text-slate-100'
                  : 'text-slate-400 dark:text-slate-500'
                }>{s.label}</span>
              </li>
            );
          })}
        </ol>

        {state.message && !state.error && (
          <div className="text-xs text-slate-500 dark:text-slate-400 truncate font-mono">{state.message}</div>
        )}
        {state.error && (
          <div className="text-xs text-rose-600 dark:text-rose-400 break-words">{state.error}</div>
        )}

        <div className="flex justify-end">
          <button
            onClick={onClose}
            disabled={!closeable}
            className="px-3 py-1.5 rounded-lg text-xs font-medium text-white bg-slate-600 hover:bg-slate-700 disabled:opacity-50"
          >{t('pp.ollamaClose')}</button>
        </div>
      </div>
    </div>
  );
}

// ── Ollama model browser modal ────────────────────────────────────────

function ModelBrowserModal({
  open, onClose, systemInfo, sttBackend, sttModel, installedModels, pullState, onPull, onDelete,
}: {
  open: boolean;
  onClose: () => void;
  systemInfo: import('@/lib/types').SystemInfo | null;
  sttBackend: string;
  sttModel: string;
  installedModels: string[];
  pullState: PullState;
  onPull: (tag: string) => void;
  onDelete: (tag: string) => void;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<OllamaCategory | 'all'>('all');
  const [registry, setRegistry] = useState<{ loading: boolean; results: import('@/lib/types').OllamaRegistryEntry[]; error: string | null }>({
    loading: false, results: [], error: null,
  });

  // Debounce live registry search.
  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (q.length < 2) {
      setRegistry({ loading: false, results: [], error: null });
      return;
    }
    setRegistry((r) => ({ ...r, loading: true, error: null }));
    const id = setTimeout(async () => {
      try {
        const results = await api.ollamaSearchRegistry(q);
        setRegistry({ loading: false, results, error: null });
      } catch (e) {
        setRegistry({ loading: false, results: [], error: String(e) });
      }
    }, 350);
    return () => clearTimeout(id);
  }, [query, open]);

  if (!open) return null;

  const ramMb = systemInfo?.total_ram_mb ?? 0;
  const cpuCores = systemInfo?.cpu_cores ?? 0;
  const concurrentLoad = sttBackend === 'whisper-local' ? whisperWorkingSetMb(sttModel) : 0;
  const installed = new Set(installedModels);
  const results = searchCatalog(query, category);

  const platform = systemInfo?.platform ?? 'other';
  const scoreEntry = (e: OllamaCatalogEntry): ScoreResult => {
    const tps = systemInfo ? estimateLlmTokensPerSec(e, systemInfo, concurrentLoad) : null;
    return scoreCompatibility({
      total_ram_mb: ramMb,
      cpu_cores: cpuCores,
      platform,
      concurrent_load_mb: concurrentLoad,
      min_ram_mb: e.min_ram_mb,
      recommended_ram_mb: e.recommended_ram_mb,
      recommended_cores: e.recommended_cores,
      estimated_throughput_tok_s: tps ?? undefined,
      min_throughput_tok_s: LLM_MIN_TOK_S,
    });
  };

  // Suggestions: top 4 catalog entries that fit ("best"), shown when not searching.
  const suggestions = !query && category === 'all' && ramMb > 0
    ? OLLAMA_CATALOG
        .filter((e) => scoreEntry(e).tier === 'best')
        .slice(0, 4)
    : [];

  const categoryChips: { value: OllamaCategory | 'all'; labelKey: string }[] = [
    { value: 'all',          labelKey: 'pp.ollamaCatAll' },
    { value: 'fast',         labelKey: 'pp.ollamaCatFast' },
    { value: 'balanced',     labelKey: 'pp.ollamaCatBalanced' },
    { value: 'multilingual', labelKey: 'pp.ollamaCatMultilingual' },
    { value: 'quality',      labelKey: 'pp.ollamaCatQuality' },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="w-full max-w-2xl mx-4 max-h-[85vh] bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 shadow-xl flex flex-col">
        <div className="p-5 border-b border-slate-200 dark:border-slate-700 flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-base">{t('pp.ollamaBrowseTitle')}</h3>
            <button
              onClick={onClose}
              className="text-slate-400 hover:text-slate-600 dark:text-slate-500 dark:hover:text-slate-300 text-lg leading-none"
              aria-label="Close"
            >×</button>
          </div>
          <p className="text-xs text-slate-500 dark:text-slate-400">
            {ramMb > 0
              ? (concurrentLoad > 0
                  ? t('pp.ollamaBrowseSubtitleWithStt', {
                      ram: `${(ramMb / 1024).toFixed(1)} GB`,
                      stt: sttModel,
                      load: `${(concurrentLoad / 1024).toFixed(1)} GB`,
                    })
                  : t('pp.ollamaBrowseSubtitle', { ram: `${(ramMb / 1024).toFixed(1)} GB` }))
              : t('pp.ollamaBrowseSubtitleNoRam')}
          </p>
          <div className="flex items-center gap-2">
            <svg className="w-4 h-4 text-slate-400" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-4.35-4.35M11 19a8 8 0 100-16 8 8 0 000 16z"/></svg>
            <input
              type="text"
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t('pp.ollamaBrowseSearchPlaceholder')}
              className="flex-1 px-2 py-1.5 rounded border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 text-slate-700 dark:text-slate-200 text-sm focus:outline-none focus:ring-2 focus:ring-amber-500/40"
            />
          </div>
          <div className="flex flex-wrap gap-1.5">
            {categoryChips.map((c) => (
              <button
                key={c.value}
                onClick={() => setCategory(c.value)}
                className={
                  'px-2.5 py-1 rounded-full text-[11px] font-medium border transition ' +
                  (category === c.value
                    ? 'bg-amber-500 border-amber-500 text-white'
                    : 'bg-slate-50 dark:bg-slate-700 border-slate-200 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-600')
                }
              >{t(c.labelKey)}</button>
            ))}
          </div>
        </div>

        <div className="flex-1 overflow-auto p-5 flex flex-col gap-4">
          {installedModels.length > 0 && (
            <section className="flex flex-col gap-2">
              <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
                {t('pp.ollamaBrowseInstalled', { count: installedModels.length })}
              </h4>
              <div className="flex flex-col gap-2">
                {installedModels.map((tag) => {
                  const catalogEntry = OLLAMA_CATALOG.find((e) => e.tag === tag);
                  return (
                    <InstalledRow
                      key={tag}
                      tag={tag}
                      family={catalogEntry?.family ?? tag.split(':')[0]}
                      description={catalogEntry?.description ?? null}
                      onDelete={() => onDelete(tag)}
                    />
                  );
                })}
              </div>
            </section>
          )}

          {suggestions.length > 0 && (
            <section className="flex flex-col gap-2">
              <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
                {t('pp.ollamaBrowseSuggestions')}
              </h4>
              <div className="flex flex-col gap-2">
                {suggestions.map((m) => (
                  <ModelRow
                    key={`sug-${m.tag}`}
                    entry={m}
                    score={scoreEntry(m)}
                    cpuCores={cpuCores}
                    systemInfo={systemInfo}
                    concurrentLoadMb={concurrentLoad}
                    isInstalled={installed.has(m.tag)}
                    isPulling={pullState.running && pullState.model === m.tag}
                    onPull={() => onPull(m.tag)}
                    onDelete={() => onDelete(m.tag)}
                    ramMb={ramMb}
                  />
                ))}
              </div>
            </section>
          )}

          <section className="flex flex-col gap-2">
            <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
              {query ? t('pp.ollamaBrowseResults', { count: results.length }) : t('pp.ollamaBrowseAll')}
              <span className="ml-2 normal-case text-slate-400 dark:text-slate-500 font-normal">{t('pp.ollamaBrowseCuratedHint')}</span>
            </h4>
            {results.length === 0 ? (
              <div className="text-xs text-slate-500 dark:text-slate-400 italic py-4 text-center">
                {t('pp.ollamaBrowseNoMatch')}
              </div>
            ) : (
              <div className="flex flex-col gap-2">
                {results.map((m) => (
                  <ModelRow
                    key={m.tag}
                    entry={m}
                    score={scoreEntry(m)}
                    cpuCores={cpuCores}
                    systemInfo={systemInfo}
                    concurrentLoadMb={concurrentLoad}
                    isInstalled={installed.has(m.tag)}
                    isPulling={pullState.running && pullState.model === m.tag}
                    onPull={() => onPull(m.tag)}
                    onDelete={() => onDelete(m.tag)}
                    ramMb={ramMb}
                  />
                ))}
              </div>
            )}
          </section>

          {query.trim().length >= 2 && (
            <section className="flex flex-col gap-2">
              <h4 className="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400 flex items-center gap-2 flex-wrap">
                {t('pp.ollamaBrowseRegistry')}
                <span
                  title={t('pp.ollamaExperimentalTooltip')}
                  className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium border text-purple-700 dark:text-purple-300 bg-purple-50 dark:bg-purple-500/10 border-purple-200 dark:border-purple-500/30 normal-case tracking-normal cursor-help"
                >
                  <svg className="w-3 h-3" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24" aria-hidden="true">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M9 3h6M10 3v6L4.5 18.5a1.5 1.5 0 001.3 2.25h12.4a1.5 1.5 0 001.3-2.25L14 9V3" />
                  </svg>
                  {t('pp.ollamaExperimental')}
                </span>
                {registry.loading && (
                  <span className="inline-block w-3 h-3 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
                )}
                <span className="ml-1 normal-case text-slate-400 dark:text-slate-500 font-normal">{t('pp.ollamaBrowseRegistryHint')}</span>
              </h4>
              {registry.error && (
                <div className="text-[11px] text-rose-600 dark:text-rose-400">
                  {t('pp.ollamaBrowseRegistryError')}: {registry.error}
                </div>
              )}
              {!registry.loading && !registry.error && registry.results.length === 0 && (
                <div className="text-xs text-slate-500 dark:text-slate-400 italic py-3 text-center">
                  {t('pp.ollamaBrowseNoRegistryMatch')}
                </div>
              )}
              {registry.results.length > 0 && (
                <div className="flex flex-col gap-2">
                  {registry.results.map((m) => (
                    <RegistryRow
                      key={m.model_name}
                      entry={m}
                      installedModels={installed}
                      pullState={pullState}
                      onPull={onPull}
                      onDelete={onDelete}
                    />
                  ))}
                </div>
              )}
            </section>
          )}
        </div>
      </div>
    </div>
  );
}

function RegistryRow({
  entry, installedModels, pullState, onPull, onDelete,
}: {
  entry: import('@/lib/types').OllamaRegistryEntry;
  installedModels: Set<string>;
  pullState: PullState;
  onPull: (tag: string) => void;
  onDelete: (tag: string) => void;
}) {
  const { t } = useTranslation();
  // Default tag preference: explicit "latest" if present, otherwise first label, otherwise empty (= ":latest" by Ollama default).
  const defaultLabel = entry.labels.find((l) => l.toLowerCase() === 'latest')
    ?? entry.labels[0] ?? 'latest';
  const [selectedLabel, setSelectedLabel] = useState<string>(defaultLabel);
  const fullTag = selectedLabel ? `${entry.model_name}:${selectedLabel}` : entry.model_name;
  const isInstalled = installedModels.has(fullTag);
  const isPulling = pullState.running && pullState.model === fullTag;

  const fmtPulls = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(0)}k`;
    return `${n}`;
  };

  return (
    <div className="rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800/40 p-3 flex flex-col gap-1.5">
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0 flex flex-col gap-0.5">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">{entry.model_name}</span>
            {isInstalled && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-medium border text-sky-700 dark:text-sky-300 bg-sky-50 dark:bg-sky-500/10 border-sky-200 dark:border-sky-500/30">
                {t('pp.ollamaInstalled')}
              </span>
            )}
            <span className="px-1.5 py-0.5 rounded text-[10px] font-medium border text-slate-500 dark:text-slate-400 bg-slate-50 dark:bg-slate-700/50 border-slate-200 dark:border-slate-600">
              {t('pp.ollamaUnverified')}
            </span>
          </div>
          {entry.description && (
            <p className="text-xs text-slate-600 dark:text-slate-300 line-clamp-2">{entry.description}</p>
          )}
          <div className="text-[11px] text-slate-500 dark:text-slate-400 flex items-center gap-2 flex-wrap">
            {entry.pulls > 0 && <span>{fmtPulls(entry.pulls)} {t('pp.ollamaPulls')}</span>}
            {entry.labels.length > 0 && (
              <>
                <span>·</span>
                <label className="flex items-center gap-1.5">
                  {t('pp.ollamaTag')}
                  <select
                    value={selectedLabel}
                    onChange={(e) => setSelectedLabel(e.target.value)}
                    className="px-1.5 py-0.5 rounded border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700 text-slate-700 dark:text-slate-200 text-[11px] [color-scheme:light] dark:[color-scheme:dark] focus:outline-none focus:ring-1 focus:ring-amber-500/40"
                  >
                    {entry.labels.map((l) => (
                      <option
                        key={l}
                        value={l}
                        className="bg-white dark:bg-slate-800 text-slate-700 dark:text-slate-200"
                      >{l}</option>
                    ))}
                  </select>
                </label>
              </>
            )}
            <span className="font-mono ml-auto">{fullTag}</span>
          </div>
        </div>
        {isInstalled ? (
          <button
            onClick={() => onDelete(fullTag)}
            title={t('pp.ollamaDeleteModel')}
            aria-label={t('pp.ollamaDeleteModel')}
            className="shrink-0 p-1.5 rounded text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 hover:bg-rose-50 dark:hover:bg-rose-500/10 transition"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3"/>
            </svg>
          </button>
        ) : (
          <button
            onClick={() => onPull(fullTag)}
            disabled={isPulling}
            className={
              'shrink-0 px-3 py-1.5 rounded-lg text-xs font-medium border transition ' +
              (isPulling
                ? 'text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30 cursor-wait'
                : 'text-white bg-amber-500 hover:bg-amber-600 border-amber-500')
            }
          >
            {isPulling ? t('pp.ollamaPullProgress') : t('pp.ollamaDownload')}
          </button>
        )}
      </div>
    </div>
  );
}

function InstalledRow({ tag, family, description, onDelete }: {
  tag: string;
  family: string;
  description: string | null;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="rounded-lg border border-emerald-200 dark:border-emerald-500/30 bg-emerald-50/50 dark:bg-emerald-500/5 p-3 flex items-start gap-3">
      <div className="flex-1 min-w-0 flex flex-col gap-0.5">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">{family}</span>
          <span className="text-[11px] font-mono text-slate-500 dark:text-slate-400">{tag}</span>
          <span className="px-1.5 py-0.5 rounded text-[10px] font-medium border text-emerald-700 dark:text-emerald-300 bg-emerald-100 dark:bg-emerald-500/20 border-emerald-200 dark:border-emerald-500/40">
            {t('pp.ollamaInstalled')}
          </span>
        </div>
        {description && <p className="text-xs text-slate-600 dark:text-slate-300">{description}</p>}
      </div>
      <button
        onClick={onDelete}
        title={t('pp.ollamaDeleteModel')}
        aria-label={t('pp.ollamaDeleteModel')}
        className="shrink-0 p-1.5 rounded text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 hover:bg-rose-50 dark:hover:bg-rose-500/10 transition"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3"/>
        </svg>
      </button>
    </div>
  );
}

function ModelRow({
  entry, score, cpuCores, systemInfo, concurrentLoadMb, isInstalled, isPulling, onPull, onDelete, ramMb,
}: {
  entry: OllamaCatalogEntry;
  score: ScoreResult;
  cpuCores: number;
  systemInfo: import('@/lib/types').SystemInfo | null;
  concurrentLoadMb: number;
  isInstalled: boolean;
  isPulling: boolean;
  onPull: () => void;
  onDelete: () => void;
  ramMb: number;
}) {
  const { t } = useTranslation();
  const tier = score.tier;
  const tokensPerSec = systemInfo ? estimateLlmTokensPerSec(entry, systemInfo, concurrentLoadMb) : null;

  const badge = (() => {
    if (ramMb === 0) return null;
    const styles: Record<Tier, string> = {
      best:      'text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-500/10 border-emerald-200 dark:border-emerald-500/30',
      fits:      'text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-500/10 border-emerald-200 dark:border-emerald-500/30',
      tight:     'text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30',
      too_large: 'text-rose-700 dark:text-rose-300 bg-rose-50 dark:bg-rose-500/10 border-rose-200 dark:border-rose-500/30',
    };
    const label: Record<Tier, string> = {
      best:      t('pp.ollamaSuitBest'),
      fits:      t('pp.ollamaSuitFits'),
      tight:     t('pp.ollamaSuitTight'),
      too_large: t('pp.ollamaSuitTooLarge'),
    };
    return (
      <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium border ${styles[tier]}`}>
        {label[tier]}
      </span>
    );
  })();

  const warn = (() => {
    if (ramMb === 0) return null;
    if (score.reason === 'throughput' && tokensPerSec != null) {
      return t('pp.ollamaWarnSlowThroughput', { tps: tokensPerSec.toFixed(0) });
    }
    if (score.reason === 'cpu') {
      return t('pp.ollamaWarnSlowCpu', { cores: cpuCores });
    }
    if (tier === 'tight') {
      return t('pp.ollamaWarnTight', { rec: `${(entry.recommended_ram_mb / 1024).toFixed(0)} GB` });
    }
    if (tier === 'too_large') {
      return t('pp.ollamaWarnTooLarge', { min: `${(entry.min_ram_mb / 1024).toFixed(0)} GB` });
    }
    return null;
  })();

  return (
    <div className="rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40 p-3 flex flex-col gap-1.5">
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0 flex flex-col gap-0.5">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">{entry.family}</span>
            <span className="text-[11px] font-mono text-slate-500 dark:text-slate-400">{entry.tag}</span>
            {badge}
            {isInstalled && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-medium border text-sky-700 dark:text-sky-300 bg-sky-50 dark:bg-sky-500/10 border-sky-200 dark:border-sky-500/30">
                {t('pp.ollamaInstalled')}
              </span>
            )}
          </div>
          <p className="text-xs text-slate-600 dark:text-slate-300">{entry.description}</p>
          <div className="text-[11px] text-slate-500 dark:text-slate-400 flex items-center gap-2 flex-wrap">
            <span>{entry.params}</span>
            <span>·</span>
            <span>{fmtSize(entry.size_mb)}</span>
            <span>·</span>
            <span>{t('pp.ollamaRecRam')} {(entry.recommended_ram_mb / 1024).toFixed(0)} GB</span>
            {tokensPerSec != null && (
              <>
                <span>·</span>
                <span className="font-mono tabular-nums">{fmtTokensPerSec(tokensPerSec)}</span>
              </>
            )}
          </div>
          {warn && (
            <div className={
              'text-[11px] mt-0.5 ' +
              (tier === 'too_large' ? 'text-rose-600 dark:text-rose-400' : 'text-amber-600 dark:text-amber-400')
            }>{warn}</div>
          )}
        </div>
        {isInstalled ? (
          <button
            onClick={onDelete}
            title={t('pp.ollamaDeleteModel')}
            aria-label={t('pp.ollamaDeleteModel')}
            className="shrink-0 p-1.5 rounded text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 hover:bg-rose-50 dark:hover:bg-rose-500/10 transition"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3"/>
            </svg>
          </button>
        ) : (
          <button
            onClick={onPull}
            disabled={isPulling}
            className={
              'shrink-0 px-3 py-1.5 rounded-lg text-xs font-medium border transition ' +
              (isPulling
                ? 'text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30 cursor-wait'
                : 'text-white bg-amber-500 hover:bg-amber-600 border-amber-500')
            }
          >
            {isPulling ? t('pp.ollamaPullProgress') : t('pp.ollamaDownload')}
          </button>
        )}
      </div>
    </div>
  );
}
