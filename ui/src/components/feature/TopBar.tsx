import { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { api } from '@/lib/tauri';
import type { Config, LlmModelInfo, ModelInfo } from '@/lib/types';
import Select from '@/components/ui/Select';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { dismissError, hotkeyPaused, hotkeyResumed } from '@/store/slices/sttSlice';

interface TopBarProps {
  title: string;
  subtitle?: string;
}

const PRESETS = [
  { key: 'local', labelKey: 'topbar.preset.local', backend: 'whisper-local', ppProvider: 'ollama' },
  { key: 'openai', labelKey: 'topbar.preset.openai', backend: 'openai', ppProvider: 'openai' },
  { key: 'deepgram', labelKey: 'topbar.preset.deepgram', backend: 'deepgram', ppProvider: '' },
  { key: 'smallest', labelKey: 'topbar.preset.smallest', backend: 'smallest', ppProvider: '' },
];

const STT_MODELS: Record<string, { value: string; label: string }[]> = {
  'whisper-local': [
    { value: 'tiny', label: 'tiny' }, { value: 'tiny.en', label: 'tiny.en' },
    { value: 'base', label: 'base' }, { value: 'base.en', label: 'base.en' },
    { value: 'small', label: 'small' }, { value: 'small.en', label: 'small.en' },
    { value: 'medium', label: 'medium' }, { value: 'medium.en', label: 'medium.en' },
    { value: 'large-v3', label: 'large-v3' },
  ],
  openai: [
    { value: 'whisper-1', label: 'whisper-1' },
    { value: 'gpt-4o-transcribe', label: 'gpt-4o-transcribe' },
    { value: 'gpt-4o-mini-transcribe', label: 'gpt-4o-mini-transcribe' },
  ],
  deepgram: [
    { value: 'nova-3', label: 'nova-3' }, { value: 'nova-2', label: 'nova-2' },
    { value: 'nova-2-general', label: 'nova-2-general' }, { value: 'nova-2-meeting', label: 'nova-2-meeting' },
    { value: 'nova-2-phonecall', label: 'nova-2-phonecall' }, { value: 'enhanced', label: 'enhanced' },
    { value: 'base', label: 'base' },
  ],
  smallest: [
    { value: 'pulse', label: 'pulse' },
  ],
};

const PP_OPENAI_MODELS = [
  { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
  { value: 'gpt-4o', label: 'gpt-4o' },
  { value: 'gpt-4.1-nano', label: 'gpt-4.1-nano' },
  { value: 'gpt-4.1-mini', label: 'gpt-4.1-mini' },
  { value: 'gpt-4.1', label: 'gpt-4.1' },
];

function activePreset(config: Config): string {
  const backend = config.general.backend;
  const pp = config.post_processing;
  if (backend === 'whisper-local' && pp.provider === 'ollama') return 'local';
  if (backend === 'openai' && pp.provider === 'openai') return 'openai';
  if (backend === 'deepgram' && !pp.enabled) return 'deepgram';
  if (backend === 'smallest' && !pp.enabled) return 'smallest';
  return 'custom';
}

type PresetSnapshot = Pick<Config['post_processing'], 'enabled' | 'provider' | 'model' | 'ollama_model'> & {
  stt_model?: string;
};

const PRESET_SNAPSHOT_KEY = 'verbatim.presetSnapshots';

function loadSnapshots(): Record<string, PresetSnapshot> {
  try {
    const raw = localStorage.getItem(PRESET_SNAPSHOT_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function saveSnapshots(snapshots: Record<string, PresetSnapshot>) {
  try {
    localStorage.setItem(PRESET_SNAPSHOT_KEY, JSON.stringify(snapshots));
  } catch {
    /* ignore quota/availability errors */
  }
}

function snapshotFromConfig(c: Config): PresetSnapshot {
  return {
    enabled: c.post_processing.enabled,
    provider: c.post_processing.provider,
    model: c.post_processing.model,
    ollama_model: c.post_processing.ollama_model,
    stt_model: sttModelValue(c),
  };
}

const BACKEND_OPTIONS = [
  { value: 'whisper-local', labelKey: 'topbar.preset.local' },
  { value: 'openai', labelKey: 'topbar.preset.openai' },
  { value: 'deepgram', labelKey: 'topbar.preset.deepgram' },
  { value: 'smallest', labelKey: 'topbar.preset.smallest' },
];

const PP_PROVIDER_OPTIONS = [
  { value: 'openai', labelKey: 'topbar.preset.openai' },
  { value: 'ollama', labelKey: 'pp.ollama' },
];

function sttModelValue(config: Config): string {
  switch (config.general.backend) {
    case 'whisper-local': return config.whisper.model;
    case 'openai': return config.openai.model;
    case 'deepgram': return config.deepgram.model;
    case 'smallest': return 'pulse';
    default: return '';
  }
}

function checkAvailability(
  key: string,
  config: Config,
  whisperModels: ModelInfo[],
  llmModels: LlmModelInfo[],
  t: (key: string) => string,
): string | null {
  if (key === 'local') {
    const hasModel = whisperModels.some((m) => m.downloaded);
    if (!hasModel) return t('topbar.noWhisperModel');
    const hasLlm = llmModels.some((m) => m.downloaded);
    if (!hasLlm && config.post_processing.enabled) return t('topbar.noLlmForPP');
    return null;
  }
  if (key === 'openai') {
    if (!config.openai.api_key) return t('topbar.openaiKeyRequired');
    return null;
  }
  if (key === 'deepgram') {
    if (!config.deepgram.api_key) return t('topbar.deepgramKeyRequired');
    return null;
  }
  if (key === 'smallest') {
    if (!config.smallest.api_key) return t('topbar.smallestKeyRequired');
    return null;
  }
  return null;
}

function CustomModal({
  draft,
  setDraft,
  whisperModels,
  llmModels,
  onSave,
  onCancel,
}: {
  draft: Config;
  setDraft: (c: Config | null) => void;
  whisperModels: ModelInfo[];
  llmModels: LlmModelInfo[];
  onSave: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();

  const update = (fn: (c: Config) => void) => {
    const next = structuredClone(draft);
    fn(next);
    setDraft(next);
  };

  const backend = draft.general.backend;
  const ppEnabled = draft.post_processing.enabled;
  const ppProvider = draft.post_processing.provider;

  const hasOpenaiKey = !!draft.openai.api_key;
  const hasDeepgramKey = !!draft.deepgram.api_key;
  const hasSmallestKey = !!draft.smallest.api_key;
  const downloadedWhisper = whisperModels.filter((m) => m.downloaded);
  const downloadedLlm = llmModels.filter((m) => m.downloaded);

  // Backend availability
  const backendAvailability: Record<string, string | null> = {
    'whisper-local': downloadedWhisper.length === 0 ? t('topbar.downloadWhisperFirst') : null,
    openai: !hasOpenaiKey ? t('topbar.addOpenaiKeyFirst') : null,
    deepgram: !hasDeepgramKey ? t('topbar.addDeepgramKeyFirst') : null,
    smallest: !hasSmallestKey ? t('topbar.addSmallestKeyFirst') : null,
  };

  // Filter backend options: available ones are selectable, unavailable show reason
  const backendOptions = BACKEND_OPTIONS.map((o) => ({
    value: o.value,
    label: backendAvailability[o.value] ? `${t(o.labelKey)} — ${backendAvailability[o.value]}` : t(o.labelKey),
    disabled: !!backendAvailability[o.value],
  }));

  const sttModel = backend === 'whisper-local' ? draft.whisper.model
    : backend === 'openai' ? draft.openai.model
    : backend === 'deepgram' ? draft.deepgram.model
    : 'pulse'; // smallest — single fixed model

  // For whisper-local, only show downloaded models
  const sttModelOptions = backend === 'whisper-local'
    ? (STT_MODELS['whisper-local'] || []).filter((o) => downloadedWhisper.some((m) => m.name === o.value))
    : (STT_MODELS[backend] || []);

  const ppModel = ppProvider === 'openai' ? draft.post_processing.model : draft.post_processing.ollama_model;

  // PP provider availability
  const ppProviderAvailability: Record<string, string | null> = {
    openai: !hasOpenaiKey ? t('topbar.addOpenaiKeyShort') : null,
    ollama: downloadedLlm.length === 0 ? t('topbar.downloadLlmFirst') : null,
  };

  const ppProviderOptions = PP_PROVIDER_OPTIONS.map((o) => ({
    value: o.value,
    label: ppProviderAvailability[o.value] ? `${t(o.labelKey)} — ${ppProviderAvailability[o.value]}` : t(o.labelKey),
    disabled: !!ppProviderAvailability[o.value],
  }));

  // For local LLM, only show downloaded models
  const ppModelOpts = ppProvider === 'openai'
    ? PP_OPENAI_MODELS
    : llmModels.filter((m) => m.downloaded).map((m) => ({ value: m.id, label: m.display_name }));

  // Auto-correct selected models if not in available options
  const sttModelValid = sttModelOptions.some((o) => o.value === sttModel);
  const ppModelValid = !ppEnabled || ppModelOpts.some((o) => o.value === ppModel);

  const sttWarning = backendAvailability[backend];
  const ppWarning = ppEnabled ? ppProviderAvailability[ppProvider] : null;

  // Block save if current selections are unavailable or selected model isn't valid
  const canSave = !sttWarning && !ppWarning && sttModelValid && ppModelValid;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center">
      <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" onClick={onCancel} />
      <div className="relative bg-white dark:bg-slate-800 rounded-2xl border border-slate-200 dark:border-slate-700 shadow-2xl w-full max-w-md mx-4 p-6 flex flex-col gap-5">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 flex items-center justify-center rounded-lg bg-amber-50 dark:bg-amber-500/10 border border-amber-100 dark:border-amber-500/20">
              <i className="ri-equalizer-line text-amber-500 text-base" />
            </div>
            <div>
              <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('topbar.customConfig')}</h2>
              <p className="text-slate-400 dark:text-slate-500 text-[11px]">{t('topbar.customConfigDesc')}</p>
            </div>
          </div>
          <button
            type="button"
            onClick={onCancel}
            className="w-7 h-7 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors cursor-pointer"
          >
            <i className="ri-close-line text-base" />
          </button>
        </div>

        {/* STT Backend */}
        <div className="flex flex-col gap-1.5">
          <label className="text-slate-600 dark:text-slate-300 text-xs font-medium">{t('topbar.sttBackend')}</label>
          <Select
            value={backend}
            onChange={(val) => update((c) => { c.general.backend = val; })}
            options={backendOptions}
          />
          {sttWarning && (
            <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1">
              <i className="ri-error-warning-line text-xs" />{sttWarning}
            </p>
          )}
        </div>

        {/* STT Model */}
        <div className="flex flex-col gap-1.5">
          <label className="text-slate-600 dark:text-slate-300 text-xs font-medium">{t('topbar.sttModel')}</label>
          {sttModelOptions.length > 0 ? (
            <>
              <Select
                value={sttModel}
                onChange={(val) => update((c) => {
                  switch (c.general.backend) {
                    case 'whisper-local': c.whisper.model = val; break;
                    case 'openai': c.openai.model = val; break;
                    case 'deepgram': c.deepgram.model = val; break;
                    // smallest: single fixed model — nothing to persist
                  }
                })}
                options={sttModelOptions}
                placeholder={t('topbar.selectModel')}
              />
              {!sttModelValid && (
                <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1">
                  <i className="ri-error-warning-line text-xs" />
                  {t('topbar.modelNotDownloaded', { model: sttModel })}
                </p>
              )}
            </>
          ) : (
            <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1 py-2">
              <i className="ri-error-warning-line text-xs" />
              {backend === 'whisper-local' ? t('topbar.noWhisperModels') : t('topbar.noModelsAvailable')}
            </p>
          )}
        </div>

        {/* Divider */}
        <div className="h-px bg-slate-100 dark:bg-slate-700" />

        {/* Post-Processing Toggle */}
        <div className="flex items-center justify-between">
          <div>
            <p className="text-slate-600 dark:text-slate-300 text-xs font-medium">{t('topbar.ppToggle')}</p>
            <p className="text-slate-400 dark:text-slate-500 text-[11px]">{t('topbar.ppToggleDesc')}</p>
          </div>
          <button
            type="button"
            onClick={() => update((c) => { c.post_processing.enabled = !c.post_processing.enabled; })}
            className={`relative w-9 h-5 rounded-full transition-colors cursor-pointer ${
              ppEnabled ? 'bg-amber-500' : 'bg-slate-300 dark:bg-slate-600'
            }`}
          >
            <span className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
              ppEnabled ? 'translate-x-4' : ''
            }`} />
          </button>
        </div>

        {/* PP Provider + Model (shown when enabled) */}
        {ppEnabled && (
          <div className="flex flex-col gap-3 pl-0">
            <div className="flex flex-col gap-1.5">
              <label className="text-slate-600 dark:text-slate-300 text-xs font-medium">{t('topbar.ppProvider')}</label>
              <Select
                value={ppProvider}
                onChange={(val) => update((c) => { c.post_processing.provider = val; })}
                options={ppProviderOptions}
              />
              {ppWarning && (
                <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1">
                  <i className="ri-error-warning-line text-xs" />{ppWarning}
                </p>
              )}
            </div>
            <div className="flex flex-col gap-1.5">
              <label className="text-slate-600 dark:text-slate-300 text-xs font-medium">{t('topbar.ppModel')}</label>
              {ppModelOpts.length > 0 ? (
                <>
                  <Select
                    value={ppModel}
                    onChange={(val) => update((c) => {
                      if (c.post_processing.provider === 'openai') {
                        c.post_processing.model = val;
                      } else {
                        c.post_processing.ollama_model = val;
                      }
                    })}
                    options={ppModelOpts}
                    placeholder={t('topbar.selectModel')}
                  />
                  {!ppModelValid && (
                    <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1">
                      <i className="ri-error-warning-line text-xs" />
                      {t('topbar.modelNotDownloaded', { model: ppModel })}
                    </p>
                  )}
                </>
              ) : (
                <p className="text-amber-600 dark:text-amber-400 text-[11px] flex items-center gap-1 py-2">
                  <i className="ri-error-warning-line text-xs" />
                  {ppProvider === 'ollama' ? t('topbar.noLlmModels') : t('topbar.noModelsAvailable')}
                </p>
              )}
            </div>
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm font-medium text-slate-600 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 rounded-lg transition-colors cursor-pointer"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={onSave}
            disabled={!canSave}
            className={`px-4 py-2 text-sm font-medium rounded-lg transition-colors ${
              canSave
                ? 'bg-amber-500 hover:bg-amber-600 text-white cursor-pointer'
                : 'bg-slate-200 dark:bg-slate-600 text-slate-400 dark:text-slate-500 cursor-not-allowed'
            }`}
          >
            {t('common.save')}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function TopBar({ title, subtitle }: TopBarProps) {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const appState = useAppSelector((s) => s.stt.appState);
  const isPaused = useAppSelector((s) => s.stt.isPaused);
  const ppLoading = useAppSelector((s) => s.stt.ppLoading);
  const lastError = useAppSelector((s) => s.stt.lastError);
  const config = useAppSelector((s) => s.config.data);
  const whisperModels = useAppSelector((s) => s.models.whisperModels);
  const llmModels = useAppSelector((s) => s.models.llmModels);

  const [time, setTime] = useState('');
  const [gearOpen, setGearOpen] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [customModalOpen, setCustomModalOpen] = useState(false);
  const [customDraft, setCustomDraft] = useState<Config | null>(null);
  const [ollamaDetect, setOllamaDetect] = useState<{ reachable: boolean; models: string[] } | null>(null);
  const [ollamaInstalled, setOllamaInstalled] = useState<boolean | null>(null);

  useEffect(() => {
    if (!gearOpen) return;
    if (config?.post_processing.provider !== 'ollama') return;
    let cancelled = false;
    (async () => {
      try {
        const d = await api.ollamaDetect();
        if (!cancelled) setOllamaDetect({ reachable: d.reachable, models: d.models });
      } catch {
        if (!cancelled) setOllamaDetect({ reachable: false, models: [] });
      }
      try {
        const m = await api.ollamaManagedInstalled();
        if (!cancelled) setOllamaInstalled(m);
      } catch {
        if (!cancelled) setOllamaInstalled(null);
      }
    })();
    return () => { cancelled = true; };
  }, [gearOpen, config?.post_processing.provider, config?.post_processing.ollama_mode, config?.post_processing.ollama_url]);

  useEffect(() => {
    const update = () => {
      const now = new Date();
      setTime(now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }));
    };
    update();
    const id = setInterval(update, 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (!lastError) return;
    const id = setTimeout(() => dispatch(dismissError()), 10000);
    return () => clearTimeout(id);
  }, [lastError, dispatch]);

  useEffect(() => {
    if (!gearOpen && !dropdownOpen) return;
    const handle = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setGearOpen(false);
        setDropdownOpen(false);
        setWarning(null);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { setGearOpen(false); setDropdownOpen(false); setWarning(null); }
    };
    window.addEventListener('mousedown', handle);
    window.addEventListener('keydown', handleKey);
    return () => { window.removeEventListener('mousedown', handle); window.removeEventListener('keydown', handleKey); };
  }, [gearOpen, dropdownOpen]);

  // Auto-dismiss warning after 4 seconds
  useEffect(() => {
    if (!warning) return;
    const id = setTimeout(() => setWarning(null), 4000);
    return () => clearTimeout(id);
  }, [warning]);

  const isRecording = appState === 'Recording';
  const isProcessing = appState === 'Processing';
  const isIdle = appState === 'Idle';

  const handleTogglePause = () => {
    if (!isIdle) return;
    if (isPaused) {
      api.resumeHotkey();
      dispatch(hotkeyResumed());
    } else {
      api.pauseHotkey();
      dispatch(hotkeyPaused());
    }
  };

  const updateConfig = (fn: (c: Config) => void) => {
    if (!config) return;
    const next = structuredClone(config);
    fn(next);
    dispatch(saveConfig(next));
  };

  const handlePreset = (preset: typeof PRESETS[number]) => {
    if (!config) return;
    const issue = checkAvailability(preset.key, config, whisperModels, llmModels, t);
    if (issue) {
      setWarning(issue);
      return;
    }
    setWarning(null);
    setDropdownOpen(false);

    // Snapshot the leaving preset's PP settings so we can restore them later.
    const leaving = activePreset(config);
    const snapshots = loadSnapshots();
    if (leaving !== 'custom') {
      snapshots[leaving] = snapshotFromConfig(config);
    }
    const restore = snapshots[preset.key];
    saveSnapshots(snapshots);

    updateConfig((c) => {
      c.general.backend = preset.backend;
      if (restore) {
        c.post_processing.enabled = restore.enabled;
        c.post_processing.provider = restore.provider;
        c.post_processing.model = restore.model;
        c.post_processing.ollama_model = restore.ollama_model;
        if (restore.stt_model) {
          switch (preset.backend) {
            case 'whisper-local': c.whisper.model = restore.stt_model; break;
            case 'openai': c.openai.model = restore.stt_model; break;
            case 'deepgram': c.deepgram.model = restore.stt_model; break;
          }
        }
      } else if (preset.ppProvider) {
        c.post_processing.provider = preset.ppProvider;
      } else {
        c.post_processing.enabled = false;
      }
    });
  };

  const handleSttModel = (val: string) => {
    updateConfig((c) => {
      switch (c.general.backend) {
        case 'whisper-local': c.whisper.model = val; break;
        case 'openai': c.openai.model = val; break;
        case 'deepgram': c.deepgram.model = val; break;
        // smallest: single fixed model — nothing to persist
      }
    });
  };

  const handlePpModel = (val: string) => {
    updateConfig((c) => {
      // Selecting a PP model implicitly enables post-processing
      c.post_processing.enabled = true;
      if (c.post_processing.provider === 'openai') {
        c.post_processing.model = val;
      } else {
        c.post_processing.ollama_model = val;
      }
    });
  };

  const current = config ? activePreset(config) : 'local';
  const currentLabel = current === 'custom' ? t('common.custom') : (PRESETS.find((p) => p.key === current)?.labelKey ? t(PRESETS.find((p) => p.key === current)!.labelKey) : t('topbar.preset.local'));
  const isDeepgram = config?.general.backend === 'deepgram';
  const isSmallest = config?.general.backend === 'smallest';
  // Smallest is genuinely STT-only (no PP path), but Deepgram users can opt
  // into an LLM cleanup pass even though Deepgram's smart_format already
  // handles most punctuation/capitalization.
  const sttOnlyBackend = isSmallest;

  const ppModelValue = config
    ? config.post_processing.provider === 'openai'
      ? config.post_processing.model
      : config.post_processing.ollama_model
    : '';

  const ppModelOptions = config
    ? config.post_processing.provider === 'openai'
      ? PP_OPENAI_MODELS
      : llmModels.filter((m) => m.downloaded).map((m) => ({ value: m.id, label: m.display_name }))
    : [];

  return (
    <header className="h-16 min-h-[64px] bg-white dark:bg-slate-800 border-b border-slate-200 dark:border-slate-700 flex items-center px-6 gap-4">
      {/* Page Title */}
      <div className="flex-1 min-w-0">
        <h1 className="text-slate-900 dark:text-slate-100 font-semibold text-base leading-tight">{title}</h1>
        {subtitle && <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{subtitle}</p>}
      </div>

      {/* Provider Selector + Model Gear (unified) */}
      {config && (
        <div ref={containerRef} className="relative flex items-center border border-slate-200 dark:border-slate-600 rounded-lg bg-slate-50 dark:bg-slate-700 overflow-visible">
          {/* Provider dropdown button */}
          <button
            type="button"
            onClick={() => { setDropdownOpen(!dropdownOpen); setGearOpen(false); setWarning(null); }}
            className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium text-slate-700 dark:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-600 transition-colors cursor-pointer rounded-l-lg"
          >
            <i className={`${current === 'custom' ? 'ri-equalizer-line' : 'ri-mic-line'} text-sm text-amber-500`} />
            {currentLabel}
            <i className={`ri-arrow-down-s-line text-sm text-slate-400 transition-transform ${dropdownOpen ? 'rotate-180' : ''}`} />
          </button>

          {/* Divider */}
          <div className="w-px h-5 bg-slate-200 dark:bg-slate-600" />

          {/* Gear button */}
          <button
            type="button"
            onClick={() => { setGearOpen(!gearOpen); setDropdownOpen(false); setWarning(null); }}
            className={`flex items-center justify-center w-8 h-8 transition-colors cursor-pointer rounded-r-lg ${
              gearOpen
                ? 'text-amber-600 dark:text-amber-400'
                : 'text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-600'
            }`}
          >
            <i className="ri-settings-4-line text-sm" />
          </button>

          {/* Provider dropdown list */}
          {dropdownOpen && (
            <div className="absolute right-0 top-full mt-1 w-56 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl shadow-lg py-1 z-50">
              {PRESETS.map((p) => (
                <button
                  key={p.key}
                  type="button"
                  onClick={() => handlePreset(p)}
                  className={`w-full text-left px-3 py-2 text-sm transition-colors cursor-pointer flex items-center justify-between ${
                    current === p.key
                      ? 'bg-amber-50 dark:bg-amber-500/10 text-amber-700 dark:text-amber-400 font-medium'
                      : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700'
                  }`}
                >
                  {t(p.labelKey)}
                  {current === p.key && <i className="ri-check-line text-amber-500 text-sm" />}
                </button>
              ))}

              <div className="mx-2 my-1 h-px bg-slate-100 dark:bg-slate-700" />

              <button
                type="button"
                onClick={() => {
                  setDropdownOpen(false);
                  setWarning(null);
                  setCustomDraft(config ? structuredClone(config) : null);
                  setCustomModalOpen(true);
                }}
                className={`w-full text-left px-3 py-2 text-sm transition-colors cursor-pointer flex items-center justify-between ${
                  current === 'custom'
                    ? 'bg-amber-50 dark:bg-amber-500/10 text-amber-700 dark:text-amber-400 font-medium'
                    : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700'
                }`}
              >
                <span className="flex items-center gap-2">
                  <i className="ri-equalizer-line text-sm" />
                  {t('common.custom')}
                </span>
                {current === 'custom' && <i className="ri-check-line text-amber-500 text-sm" />}
              </button>

              {warning && (
                <div className="mx-2 mt-1 mb-1 px-2.5 py-2 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg">
                  <p className="text-amber-700 dark:text-amber-400 text-[11px] leading-tight flex items-start gap-1.5">
                    <i className="ri-error-warning-line text-sm flex-shrink-0 mt-px" />
                    {warning}
                  </p>
                </div>
              )}
            </div>
          )}

          {/* Model selection popup */}
          {gearOpen && (
            <div className="absolute right-0 top-full mt-1 w-72 bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl shadow-lg p-4 z-50">
              <p className="text-slate-900 dark:text-slate-100 text-xs font-semibold mb-3">{t('topbar.modelSelection')}</p>

              <div className="flex flex-col gap-3">
                <div>
                  <label className="text-slate-500 dark:text-slate-400 text-[11px] font-medium mb-1 block">{t('topbar.sttModel')}</label>
                  <Select
                    value={sttModelValue(config)}
                    onChange={handleSttModel}
                    options={
                      config.general.backend === 'whisper-local'
                        ? (STT_MODELS['whisper-local'] || []).filter((o) => whisperModels.some((m) => m.downloaded && m.name === o.value))
                        : (STT_MODELS[config.general.backend] || [])
                    }
                  />
                </div>

                {!sttOnlyBackend && (
                  <div>
                    <label className="text-slate-500 dark:text-slate-400 text-[11px] font-medium mb-1 block">{t('topbar.ppModel')}</label>
                    {(() => {
                      const isOllama = config.post_processing.provider === 'ollama';
                      let statusLabel = t('topbar.ppOff');
                      if (isOllama) {
                        const isManagedMode = config.post_processing.ollama_mode === 'managed';
                        if (isManagedMode && ollamaInstalled === false) {
                          statusLabel = t('topbar.ollamaNotInstalled');
                        } else if (ollamaDetect && !ollamaDetect.reachable) {
                          statusLabel = t('topbar.ollamaNotOn');
                        } else if (ollamaDetect && ollamaDetect.models.length === 0) {
                          statusLabel = t('topbar.ollamaNoModels');
                        } else {
                          statusLabel = t('topbar.ollamaSelectModel');
                        }
                      }
                      const modelInList = ppModelOptions.some((o) => o.value === ppModelValue);
                      const value = !config.post_processing.enabled || !modelInList ? '__status__' : ppModelValue;
                      return (
                        <Select
                          value={value}
                          onChange={(val) => {
                            if (val === '__status__') return;
                            if (val === '__off__') {
                              updateConfig((c) => { c.post_processing.enabled = false; });
                            } else {
                              handlePpModel(val);
                            }
                          }}
                          options={[
                            { value: '__status__', label: statusLabel, disabled: true },
                            ...ppModelOptions,
                            ...(config.post_processing.enabled ? [{ value: '__off__', label: t('topbar.ppTurnOff') }] : []),
                          ]}
                        />
                      );
                    })()}
                  </div>
                )}

                {isDeepgram && (
                  <p className="text-slate-400 dark:text-slate-500 text-[11px]">
                    {t('topbar.deepgramPpNote')}
                  </p>
                )}

                {isSmallest && (
                  <p className="text-slate-400 dark:text-slate-500 text-[11px]">
                    {t('topbar.smallestPpNote')}
                  </p>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Error Banner */}
      {lastError && (
        <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] bg-red-50 dark:bg-red-500/10 border border-red-200 dark:border-red-500/30 text-red-600 dark:text-red-400">
          <i className="ri-error-warning-line text-xs flex-shrink-0" />
          <span className="truncate max-w-[240px]">{lastError}</span>
          <button type="button" onClick={() => dispatch(dismissError())} className="ml-0.5 hover:text-red-800 dark:hover:text-red-300">
            <i className="ri-close-line text-xs" />
          </button>
        </div>
      )}

      {/* Recording Status Indicator */}
      <button
        type="button"
        onClick={handleTogglePause}
        disabled={!isIdle}
        className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium border whitespace-nowrap transition-colors ${
          isRecording
            ? 'bg-red-50 dark:bg-red-500/10 border-red-200 dark:border-red-500/30 text-red-600 dark:text-red-400'
            : isProcessing
            ? 'bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30 text-amber-600 dark:text-amber-400'
            : ppLoading
            ? 'bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30 text-amber-600 dark:text-amber-400'
            : isPaused
            ? 'bg-slate-100 dark:bg-slate-700 border-slate-300 dark:border-slate-600 text-slate-400 cursor-pointer hover:bg-slate-150'
            : 'bg-slate-50 dark:bg-slate-700 border-slate-200 dark:border-slate-600 text-slate-500 dark:text-slate-400 cursor-pointer hover:bg-slate-100 dark:hover:bg-slate-600'
        } ${!isIdle ? 'cursor-default' : ''}`}
      >
        <span
          className={`w-2 h-2 rounded-full ${
            isRecording ? 'bg-red-500 animate-pulse'
            : isProcessing ? 'bg-amber-500 animate-pulse'
            : ppLoading ? 'bg-amber-500 animate-pulse'
            : isPaused ? 'bg-slate-400'
            : 'bg-emerald-400'
          }`}
        />
        {isRecording ? t('topbar.recording') : isProcessing ? t('topbar.processing') : ppLoading ? t('topbar.loadingModel') : isPaused ? t('topbar.paused') : t('topbar.ready')}
      </button>

      {/* Divider */}
      <div className="h-6 w-px bg-slate-200 dark:bg-slate-700" />

      {/* Time */}
      <div className="text-slate-400 dark:text-slate-500 text-sm font-medium tabular-nums">{time}</div>

      {/* Custom Configuration Modal */}
      {customModalOpen && customDraft && (
        <CustomModal
          draft={customDraft}
          setDraft={setCustomDraft}
          whisperModels={whisperModels}
          llmModels={llmModels}
          onSave={() => {
            dispatch(saveConfig(customDraft));
            setCustomModalOpen(false);
            setCustomDraft(null);
          }}
          onCancel={() => {
            setCustomModalOpen(false);
            setCustomDraft(null);
          }}
        />
      )}
    </header>
  );
}
