import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { api, onModelDownloadProgress } from '@/lib/tauri';
import type { Config, MacPermissions, ModelDownloadProgress, ModelInfo } from '@/lib/types';
import Select from '@/components/ui/Select';

const isMac = navigator.userAgent.includes('Mac');
const isLinux = !isMac && navigator.userAgent.includes('Linux');

const LINUX_INPUT_GROUP_CMD = 'sudo usermod -aG input $USER';

type Step = 'permissions' | 'stt' | 'postproc' | 'done';
type SttChoice = 'whisper-local' | 'openai' | 'deepgram' | 'smallest';
type PpChoice = 'disabled' | 'openai' | 'ollama-later';

const DEFAULT_LOCAL_MODEL = 'base.en';

type RowKey = 'accessibility' | 'microphone' | 'input-monitoring' | 'automation' | 'linux-input';

interface Row {
  key: RowKey;
  titleKey: string;
  descKey: string;
  icon: string;
  granted: boolean;
  required: boolean;
  pane: string | null;
}

export default function Onboarding() {
  const navigate = useNavigate();
  const { t } = useTranslation();

  // ── Permissions state ────────────────────────────────────
  const [perms, setPerms] = useState<MacPermissions | null>(null);
  const [linuxInputOk, setLinuxInputOk] = useState<boolean>(true);
  const [checking, setChecking] = useState(false);
  const [copied, setCopied] = useState(false);

  // ── STT step state ───────────────────────────────────────
  const [sttChoice, setSttChoice] = useState<SttChoice>('whisper-local');
  const [openaiKey, setOpenaiKey] = useState('');
  const [deepgramKey, setDeepgramKey] = useState('');
  const [smallestKey, setSmallestKey] = useState('');
  const [whisperModels, setWhisperModels] = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>(DEFAULT_LOCAL_MODEL);
  const [downloadProgress, setDownloadProgress] = useState<ModelDownloadProgress | null>(null);

  // ── PP step state ────────────────────────────────────────
  const [ppChoice, setPpChoice] = useState<PpChoice>('disabled');

  // ── Wizard state ─────────────────────────────────────────
  const [step, setStep] = useState<Step>('permissions');
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [skipConfirm, setSkipConfirm] = useState(false);

  // ── Permissions ──────────────────────────────────────────
  const refresh = useCallback(async () => {
    setChecking(true);
    try {
      const [mac, linux] = await Promise.all([
        api.checkMacPermissions(),
        api.checkLinuxInputPermission(),
      ]);
      setPerms(mac);
      setLinuxInputOk(linux);
    } catch {
      /* leave previous values */
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  // ── Models: list & subscribe to download progress ────────
  useEffect(() => {
    api.listModels().then(setWhisperModels).catch(() => {});
  }, []);

  useEffect(() => {
    const stop = onModelDownloadProgress((p) => {
      setDownloadProgress(p.done ? null : p);
      if (p.done && !p.cancelled && !p.error) {
        // Refresh listing so the model flips to `downloaded`, and adopt the
        // freshly-downloaded model as the wizard's selection so the user
        // doesn't have to re-pick it on the next step.
        api.listModels().then(setWhisperModels).catch(() => {});
        if (p.model) setSelectedModel(p.model);
      }
    });
    return () => { stop.then((fn) => fn()).catch(() => {}); };
  }, []);

  const selectedModelInfo = useMemo(
    () => whisperModels.find((m) => m.name === selectedModel),
    [whisperModels, selectedModel]
  );
  const isDownloadingSelected = downloadProgress?.model === selectedModel;
  const downloadPct = downloadProgress && downloadProgress.total > 0
    ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100)
    : 0;

  // ── Persist config + finish ──────────────────────────────
  const persistAndExit = useCallback(async () => {
    setSaving(true);
    setSaveError(null);
    try {
      const cfg: Config = await api.getConfig();
      cfg.general.backend = sttChoice;
      // We don't write API keys unless the user actually typed one — leaving
      // them as empty strings preserves the default config so a user who
      // skipped this step doesn't accidentally wipe a previously-stored key.
      if (sttChoice === 'openai' && openaiKey) cfg.openai.api_key = openaiKey;
      if (sttChoice === 'deepgram' && deepgramKey) cfg.deepgram.api_key = deepgramKey;
      if (sttChoice === 'smallest' && smallestKey) cfg.smallest.api_key = smallestKey;
      if (sttChoice === 'whisper-local') cfg.whisper.model = selectedModel;

      if (ppChoice === 'disabled') {
        cfg.post_processing.enabled = false;
      } else if (ppChoice === 'openai') {
        cfg.post_processing.enabled = true;
        cfg.post_processing.provider = 'openai';
        if (openaiKey) cfg.openai.api_key = openaiKey;
      } else {
        // ollama-later: leave PP off; user finishes setup in Settings.
        cfg.post_processing.enabled = false;
      }

      cfg.general.onboarding_complete = true;
      await api.saveConfig(cfg);
      navigate('/', { replace: true });
    } catch (e) {
      console.error('save failed', e);
      setSaveError(String(e));
      setSaving(false);
    }
  }, [sttChoice, openaiKey, deepgramKey, smallestKey, selectedModel, ppChoice, navigate]);

  // ── Skip — minimum required: just flip the flag ──────────
  const skip = useCallback(async () => {
    try {
      const cfg = await api.getConfig();
      cfg.general.onboarding_complete = true;
      await api.saveConfig(cfg);
    } catch {/* non-fatal */}
    navigate('/', { replace: true });
  }, [navigate]);

  const openSettings = useCallback((pane: string) => {
    api.openMacSettings(pane).catch(() => {});
  }, []);

  const copyLinuxCmd = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(LINUX_INPUT_GROUP_CMD);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {/* noop */}
  }, []);

  // ── Rows for permissions step ────────────────────────────
  const rows: Row[] = [];
  if (isMac && perms) {
    rows.push(
      { key: 'accessibility', titleKey: 'onboarding.accessibility', descKey: 'onboarding.accessibilityDesc', icon: 'ri-keyboard-line', granted: perms.accessibility, required: true, pane: 'accessibility' },
      { key: 'microphone', titleKey: 'onboarding.microphone', descKey: 'onboarding.microphoneDesc', icon: 'ri-mic-line', granted: perms.microphone, required: true, pane: 'microphone' },
      { key: 'input-monitoring', titleKey: 'onboarding.inputMonitoring', descKey: 'onboarding.inputMonitoringDesc', icon: 'ri-radar-line', granted: perms.input_monitoring, required: true, pane: 'input-monitoring' },
      { key: 'automation', titleKey: 'onboarding.automation', descKey: 'onboarding.automationDesc', icon: 'ri-terminal-window-line', granted: perms.automation, required: false, pane: 'automation' },
    );
  }
  if (isLinux) {
    rows.push({ key: 'linux-input', titleKey: 'onboarding.linuxInput', descKey: 'onboarding.linuxInputDesc', icon: 'ri-keyboard-line', granted: linuxInputOk, required: true, pane: null });
  }
  const permissionsRequiredOk = rows.filter((r) => r.required).every((r) => r.granted);

  // STT step gates: local needs a downloaded model (or skip), cloud needs a key (or skip).
  const sttRequirementMet = (() => {
    if (sttChoice === 'whisper-local') return selectedModelInfo?.downloaded === true;
    if (sttChoice === 'openai') return openaiKey.trim().length > 0;
    if (sttChoice === 'deepgram') return deepgramKey.trim().length > 0;
    if (sttChoice === 'smallest') return smallestKey.trim().length > 0;
    return false;
  })();

  const stepOrder: Step[] = ['permissions', 'stt', 'postproc', 'done'];
  const stepIndex = stepOrder.indexOf(step);

  // ── Render ───────────────────────────────────────────────
  return (
    <div className="flex h-screen items-center justify-center bg-[#f8f9fa] dark:bg-slate-900 overflow-y-auto" style={{ fontFamily: "'Inter', sans-serif" }}>
      <div className="w-full max-w-lg mx-auto px-6 py-10">
        <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 p-8 shadow-sm">

          {/* Step indicator */}
          <div className="flex items-center gap-1.5 mb-6">
            {stepOrder.map((s, i) => (
              <div
                key={s}
                className={`h-1 flex-1 rounded-full transition-colors ${
                  i <= stepIndex ? 'bg-amber-500' : 'bg-slate-200 dark:bg-slate-700'
                }`}
              />
            ))}
          </div>

          {/* ── Step: Permissions ── */}
          {step === 'permissions' && (
            <>
              <div className="w-14 h-14 bg-amber-50 dark:bg-amber-500/10 rounded-2xl flex items-center justify-center mb-5">
                <i className={`${permissionsRequiredOk ? 'ri-check-double-line text-emerald-500' : 'ri-shield-check-line text-amber-500'} text-2xl`} />
              </div>
              <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-1">
                {permissionsRequiredOk ? t('onboarding.allSet') : t('onboarding.checklistTitle')}
              </h1>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">
                {permissionsRequiredOk ? t('onboarding.allSetDesc') : t('onboarding.checklistDesc')}
              </p>

              {rows.length === 0 && (
                <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">
                  {t('onboarding.allSetDesc')}
                </p>
              )}

              <div className="flex flex-col gap-3 mb-6">
                {rows.map((row) => (
                  <div key={row.key} className={`rounded-xl border p-4 transition-all ${row.granted ? 'bg-emerald-50/60 dark:bg-emerald-500/5 border-emerald-200 dark:border-emerald-500/20' : 'bg-slate-50 dark:bg-slate-900/40 border-slate-200 dark:border-slate-700'}`}>
                    <div className="flex items-start gap-3 mb-2">
                      <div className={`w-9 h-9 rounded-lg flex items-center justify-center shrink-0 ${row.granted ? 'bg-emerald-100 dark:bg-emerald-500/20' : 'bg-slate-100 dark:bg-slate-700'}`}>
                        <i className={`${row.icon} text-lg ${row.granted ? 'text-emerald-600 dark:text-emerald-400' : 'text-slate-500 dark:text-slate-400'}`} />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 flex-wrap">
                          <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">{t(row.titleKey)}</h3>
                          {!row.required && (
                            <span className="text-[10px] font-medium uppercase tracking-wide px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400">
                              {t('onboarding.optional')}
                            </span>
                          )}
                          {row.granted ? (
                            <span className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                              <i className="ri-checkbox-circle-fill" />{t('onboarding.granted')}
                            </span>
                          ) : (
                            <span className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-slate-500 dark:text-slate-400">
                              <i className="ri-close-circle-line" />{t('onboarding.notGranted')}
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 leading-relaxed">{t(row.descKey)}</p>
                      </div>
                    </div>

                    {!row.granted && row.pane && (
                      <button onClick={() => openSettings(row.pane!)} className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-lg font-medium text-xs transition-all">
                        <i className="ri-external-link-line" />{t('onboarding.openSettings')}
                      </button>
                    )}

                    {!row.granted && row.key === 'linux-input' && (
                      <div className="space-y-2">
                        <div className="text-xs text-slate-500 dark:text-slate-400">{t('onboarding.linuxInputCommand')}</div>
                        <div className="flex items-center gap-2">
                          <code className="flex-1 px-2 py-1.5 rounded bg-slate-900 text-slate-100 text-[11px] font-mono overflow-x-auto whitespace-nowrap">{LINUX_INPUT_GROUP_CMD}</code>
                          <button onClick={copyLinuxCmd} className="px-2 py-1.5 rounded bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 text-xs font-medium">
                            <i className={copied ? 'ri-check-line' : 'ri-clipboard-line'} />
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>

              <div className="flex flex-col gap-2">
                <button onClick={refresh} disabled={checking} className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-xl font-medium text-sm transition-all disabled:opacity-50">
                  <i className={checking ? 'ri-loader-4-line animate-spin' : 'ri-refresh-line'} />{t('onboarding.checkPermission')}
                </button>
                <button onClick={() => setStep('stt')} disabled={!permissionsRequiredOk} className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-xl font-semibold text-sm transition-all">
                  {permissionsRequiredOk ? t('onboarding.continue') : t('onboarding.continueBlocked')}
                </button>
              </div>
            </>
          )}

          {/* ── Step: STT provider ── */}
          {step === 'stt' && (
            <>
              <div className="w-14 h-14 bg-amber-50 dark:bg-amber-500/10 rounded-2xl flex items-center justify-center mb-5">
                <i className="ri-mic-line text-amber-500 text-2xl" />
              </div>
              <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-1">{t('onboarding.sttTitle')}</h1>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">{t('onboarding.sttDesc')}</p>

              <div className="flex flex-col gap-2 mb-5">
                {([
                  { id: 'whisper-local', title: t('onboarding.sttLocal'), desc: t('onboarding.sttLocalDesc'), icon: 'ri-computer-line' },
                  { id: 'openai', title: 'OpenAI Whisper', desc: t('onboarding.sttOpenaiDesc'), icon: 'ri-cloud-line' },
                  { id: 'deepgram', title: 'Deepgram', desc: t('onboarding.sttDeepgramDesc'), icon: 'ri-cloud-line' },
                  { id: 'smallest', title: 'Smallest', desc: t('onboarding.sttSmallestDesc'), icon: 'ri-cloud-line' },
                ] as { id: SttChoice; title: string; desc: string; icon: string }[]).map((opt) => {
                  const selected = sttChoice === opt.id;
                  return (
                    <button
                      key={opt.id}
                      onClick={() => setSttChoice(opt.id)}
                      className={`text-left rounded-xl border p-3 transition-all ${selected ? 'border-amber-400 bg-amber-50/60 dark:bg-amber-500/5 dark:border-amber-500/40' : 'border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40 hover:bg-slate-100/60 dark:hover:bg-slate-700/40'}`}
                    >
                      <div className="flex items-center gap-2.5">
                        <div className={`w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 ${selected ? 'border-amber-500' : 'border-slate-300 dark:border-slate-600'}`}>
                          {selected && <div className="w-2 h-2 rounded-full bg-amber-500" />}
                        </div>
                        <i className={`${opt.icon} text-slate-500 dark:text-slate-400 text-sm`} />
                        <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">{opt.title}</span>
                      </div>
                      <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 ml-9 leading-relaxed">{opt.desc}</p>
                    </button>
                  );
                })}
              </div>

              {sttChoice === 'whisper-local' && (
                <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50/60 dark:bg-slate-900/40 p-4 mb-5">
                  <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 block mb-2">
                    {t('onboarding.sttPickModel')}
                  </label>
                  <Select
                    value={selectedModel}
                    onChange={setSelectedModel}
                    options={
                      whisperModels.length === 0
                        ? [{ value: DEFAULT_LOCAL_MODEL, label: DEFAULT_LOCAL_MODEL }]
                        : whisperModels.map((m) => ({
                            value: m.name,
                            label: m.name,
                            hint: m.downloaded ? `· ${t('onboarding.sttModelInstalled')}` : undefined,
                            disabled: !!downloadProgress,
                          }))
                    }
                  />

                  {selectedModelInfo?.downloaded ? (
                    <p className="mt-3 text-xs text-emerald-600 dark:text-emerald-400 flex items-center gap-1.5">
                      <i className="ri-checkbox-circle-fill" />{t('onboarding.sttModelReady')}
                    </p>
                  ) : isDownloadingSelected ? (
                    <div className="mt-3 space-y-2">
                      <div className="flex items-center gap-2">
                        <div className="flex-1 bg-slate-200 dark:bg-slate-700 rounded-full h-2 overflow-hidden">
                          <div className={`h-full rounded-full transition-all ${downloadProgress?.verifying ? 'bg-sky-400 animate-pulse' : 'bg-amber-400'}`} style={{ width: downloadProgress?.verifying ? '100%' : `${downloadPct}%` }} />
                        </div>
                        <span className="text-[11px] tabular-nums text-slate-500 dark:text-slate-400 w-10 text-right">
                          {downloadProgress?.verifying ? t('common.verifying') : `${downloadPct}%`}
                        </span>
                      </div>
                      {downloadProgress?.error && (
                        <p className="text-xs text-red-500">{downloadProgress.error}</p>
                      )}
                    </div>
                  ) : (
                    <button
                      onClick={() => api.downloadModel(selectedModel).catch((e) => setSaveError(String(e)))}
                      disabled={!!downloadProgress}
                      className="mt-3 w-full px-3 py-2 rounded-lg bg-amber-500 hover:bg-amber-600 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-semibold flex items-center justify-center gap-2"
                    >
                      <i className="ri-download-line" />
                      {t('onboarding.sttDownloadModel', { name: selectedModel })}
                    </button>
                  )}
                </div>
              )}

              {sttChoice !== 'whisper-local' && (
                <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50/60 dark:bg-slate-900/40 p-4 mb-5">
                  <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 block mb-2">
                    {t('onboarding.sttApiKey')}
                  </label>
                  <input
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={sttChoice === 'openai' ? openaiKey : sttChoice === 'deepgram' ? deepgramKey : smallestKey}
                    onChange={(e) => {
                      const v = e.target.value;
                      if (sttChoice === 'openai') setOpenaiKey(v);
                      else if (sttChoice === 'deepgram') setDeepgramKey(v);
                      else setSmallestKey(v);
                    }}
                    placeholder={t('onboarding.sttApiKeyPlaceholder')}
                    className="w-full px-3 py-2 text-sm font-mono rounded-lg bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-900 dark:text-slate-100"
                  />
                  <p className="text-[11px] text-slate-400 dark:text-slate-500 mt-2 leading-relaxed">
                    {t('onboarding.sttApiKeyHint')}
                  </p>
                </div>
              )}

              <div className="flex items-center gap-2">
                <button onClick={() => setStep('permissions')} className="px-4 py-2.5 rounded-xl bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 text-sm font-medium">
                  <i className="ri-arrow-left-line mr-1" />{t('onboarding.back')}
                </button>
                <button
                  onClick={() => setStep('postproc')}
                  className={`flex-1 px-4 py-3 rounded-xl text-white font-semibold text-sm transition-all ${sttRequirementMet ? 'bg-amber-500 hover:bg-amber-600' : 'bg-slate-300 dark:bg-slate-600'}`}
                >
                  {sttRequirementMet ? t('onboarding.continue') : t('onboarding.continueAnyway')}
                </button>
              </div>
            </>
          )}

          {/* ── Step: Post-processing ── */}
          {step === 'postproc' && (
            <>
              <div className="w-14 h-14 bg-amber-50 dark:bg-amber-500/10 rounded-2xl flex items-center justify-center mb-5">
                <i className="ri-magic-line text-amber-500 text-2xl" />
              </div>
              <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-1">{t('onboarding.ppTitle')}</h1>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">{t('onboarding.ppDesc')}</p>

              <div className="flex flex-col gap-2 mb-5">
                {([
                  { id: 'disabled', title: t('onboarding.ppDisabled'), desc: t('onboarding.ppDisabledDesc'), icon: 'ri-close-circle-line' },
                  { id: 'openai', title: t('onboarding.ppOpenai'), desc: t('onboarding.ppOpenaiDesc'), icon: 'ri-cloud-line' },
                  { id: 'ollama-later', title: t('onboarding.ppOllama'), desc: t('onboarding.ppOllamaDesc'), icon: 'ri-cpu-line' },
                ] as { id: PpChoice; title: string; desc: string; icon: string }[]).map((opt) => {
                  const selected = ppChoice === opt.id;
                  return (
                    <button
                      key={opt.id}
                      onClick={() => setPpChoice(opt.id)}
                      className={`text-left rounded-xl border p-3 transition-all ${selected ? 'border-amber-400 bg-amber-50/60 dark:bg-amber-500/5 dark:border-amber-500/40' : 'border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40 hover:bg-slate-100/60 dark:hover:bg-slate-700/40'}`}
                    >
                      <div className="flex items-center gap-2.5">
                        <div className={`w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 ${selected ? 'border-amber-500' : 'border-slate-300 dark:border-slate-600'}`}>
                          {selected && <div className="w-2 h-2 rounded-full bg-amber-500" />}
                        </div>
                        <i className={`${opt.icon} text-slate-500 dark:text-slate-400 text-sm`} />
                        <span className="text-sm font-semibold text-slate-900 dark:text-slate-100">{opt.title}</span>
                      </div>
                      <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 ml-9 leading-relaxed">{opt.desc}</p>
                    </button>
                  );
                })}
              </div>

              {ppChoice === 'openai' && !openaiKey && sttChoice !== 'openai' && (
                <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50/60 dark:bg-slate-900/40 p-4 mb-5">
                  <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 block mb-2">
                    {t('onboarding.ppOpenaiKey')}
                  </label>
                  <input
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={openaiKey}
                    onChange={(e) => setOpenaiKey(e.target.value)}
                    placeholder="sk-..."
                    className="w-full px-3 py-2 text-sm font-mono rounded-lg bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 text-slate-900 dark:text-slate-100"
                  />
                </div>
              )}

              {ppChoice === 'ollama-later' && (
                <div className="rounded-xl border border-sky-200 dark:border-sky-500/30 bg-sky-50/60 dark:bg-sky-500/5 p-3 mb-5">
                  <p className="text-xs text-sky-700 dark:text-sky-300 leading-relaxed">
                    <i className="ri-information-line mr-1" />
                    {t('onboarding.ppOllamaLater')}
                  </p>
                </div>
              )}

              <div className="flex items-center gap-2">
                <button onClick={() => setStep('stt')} className="px-4 py-2.5 rounded-xl bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 text-sm font-medium">
                  <i className="ri-arrow-left-line mr-1" />{t('onboarding.back')}
                </button>
                <button onClick={() => setStep('done')} className="flex-1 px-4 py-3 rounded-xl bg-amber-500 hover:bg-amber-600 text-white font-semibold text-sm">
                  {t('onboarding.continue')}
                </button>
              </div>
            </>
          )}

          {/* ── Step: Done ── */}
          {step === 'done' && (
            <>
              <div className="w-14 h-14 bg-emerald-50 dark:bg-emerald-500/10 rounded-2xl flex items-center justify-center mb-5">
                <i className="ri-rocket-2-line text-emerald-500 text-2xl" />
              </div>
              <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-1">{t('onboarding.doneTitle')}</h1>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">{t('onboarding.doneDesc')}</p>

              <div className="rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50/60 dark:bg-slate-900/40 p-4 mb-5 space-y-2">
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-500 dark:text-slate-400">{t('onboarding.summarySttBackend')}</span>
                  <span className="text-slate-800 dark:text-slate-200 font-medium">
                    {sttChoice === 'whisper-local' ? `${t('onboarding.sttLocal')} (${selectedModel})` : sttChoice}
                  </span>
                </div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-500 dark:text-slate-400">{t('onboarding.summaryPp')}</span>
                  <span className="text-slate-800 dark:text-slate-200 font-medium">
                    {ppChoice === 'disabled' ? t('onboarding.ppDisabled') : ppChoice === 'openai' ? t('onboarding.ppOpenai') : t('onboarding.ppOllamaPending')}
                  </span>
                </div>
              </div>

              {saveError && (
                <div className="rounded-lg border border-red-200 dark:border-red-500/30 bg-red-50 dark:bg-red-500/10 p-3 mb-4">
                  <p className="text-xs text-red-700 dark:text-red-300 break-words font-mono">{saveError}</p>
                </div>
              )}

              <div className="flex items-center gap-2">
                <button onClick={() => setStep('postproc')} disabled={saving} className="px-4 py-2.5 rounded-xl bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 text-sm font-medium disabled:opacity-50">
                  <i className="ri-arrow-left-line mr-1" />{t('onboarding.back')}
                </button>
                <button onClick={persistAndExit} disabled={saving} className="flex-1 px-4 py-3 rounded-xl bg-amber-500 hover:bg-amber-600 disabled:opacity-60 disabled:cursor-not-allowed text-white font-semibold text-sm">
                  {saving ? (<><i className="ri-loader-4-line animate-spin mr-1" />{t('onboarding.saving')}</>) : t('onboarding.finish')}
                </button>
              </div>
            </>
          )}
        </div>

        <div className="text-center mt-4">
          <button onClick={() => setSkipConfirm(true)} className="text-xs text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-400 transition-colors">
            {t('onboarding.skipForNow')}
          </button>
        </div>
      </div>

      {skipConfirm && (
        <div className="fixed inset-0 bg-black/40 z-50 flex items-center justify-center p-6" onClick={() => setSkipConfirm(false)}>
          <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-200 dark:border-slate-700 w-full max-w-sm shadow-xl" onClick={(e) => e.stopPropagation()}>
            <div className="p-6">
              <div className="w-10 h-10 rounded-full bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center mb-3">
                <i className="ri-error-warning-line text-amber-500 text-xl" />
              </div>
              <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-base mb-1">
                {t('onboarding.skipConfirmTitle')}
              </h3>
              <p className="text-slate-500 dark:text-slate-400 text-sm leading-relaxed">
                {t('onboarding.skipConfirmDesc')}
              </p>
            </div>
            <div className="px-6 pb-5 flex items-center gap-2 justify-end">
              <button
                onClick={() => setSkipConfirm(false)}
                className="px-4 py-2 rounded-lg text-xs font-medium bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300"
              >
                {t('onboarding.skipConfirmStay')}
              </button>
              <button
                onClick={() => { setSkipConfirm(false); skip(); }}
                className="px-4 py-2 rounded-lg text-xs font-semibold bg-amber-500 hover:bg-amber-600 text-white"
              >
                {t('onboarding.skipConfirmGo')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
