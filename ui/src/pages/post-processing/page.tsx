import { useState, useEffect } from 'react';
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
  // null = closed, '__default__' = editing default, '__new__' = creating new, string = editing saved by name
  const [editingPrompt, setEditingPrompt] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editBody, setEditBody] = useState('');
  const [editEmoji, setEditEmoji] = useState('📝');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [seeded, setSeeded] = useState(false);

  useEffect(() => {
    if (storeConfig && !config) setConfig(structuredClone(storeConfig));
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

  const update = (fn: (c: Config) => void) => {
    if (!config) return;
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
    dispatch(saveConfig(next)).then(() => setSaved(true)).catch(console.error);
  };

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
      <div className="max-w-[860px] flex flex-col gap-5">
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
              on={config.general.backend === 'deepgram' ? false : config.post_processing.enabled}
              onChange={(v) => {
                if (config.general.backend === 'deepgram') return;
                if (v && config.post_processing.provider === 'openai' && !config.openai.api_key) {
                  setKeyWarning('post-processing');
                  return;
                }
                if (v && config.post_processing.provider === 'local' && !llmModels.some((m) => m.downloaded)) {
                  setKeyWarning('no-llm-model');
                  return;
                }
                setKeyWarning(null);
                update((c) => { c.post_processing.enabled = v; });
              }}
              disabled={config.general.backend === 'deepgram'}
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
                { value: 'local', label: t('pp.localLlm') },
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

          {config.post_processing.provider === 'local' && (
            <SettingRow label={t('stt.model')} description={t('pp.modelLocalDesc')}>
              <Select
                value={config.post_processing.llm_model}
                onChange={(val) => {
                  const m = llmModels.find((m) => m.id === val);
                  if (m && !m.downloaded) {
                    setLlmModelPrompt({ id: m.id, displayName: m.display_name, downloading: false });
                    return;
                  }
                  update((c) => { c.post_processing.llm_model = val; });
                }}
                options={llmModels.map((m) => ({
                  value: m.id,
                  label: m.display_name,
                }))}
              />
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
              value={config.post_processing.prompt}
              onChange={(e) => update((c) => { c.post_processing.prompt = e.target.value; })}
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-4 py-3.5 outline-none w-full h-32 resize-y font-mono my-3"
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

        {/* LLM Models */}
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
                onChange={(e) => update((c) => { c.post_processing.prompt = e.target.value; })}
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
                      update((c) => { c.post_processing.llm_model = llmModelPrompt.id; });
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
