import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import type { Config, PasteRule, SystemInfo } from '@/lib/types';
import { scoreCompatibility, type Tier } from '@/lib/compatibility';
import { WHISPER_META } from '@/lib/whisper-catalog';
import { estimateWhisperRealtime, fmtRealtime } from '@/lib/throughput';
import HotkeyButton, { formatHotkey, hotkeysEqual, EMPTY_HOTKEY } from '../settings/HotkeyButton';
import PasteKeyButton from '../settings/PasteKeyButton';
import Select from '@/components/ui/Select';
import { SettingRow, Toggle } from '../settings/components/SettingRow';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { fetchWhisperModels } from '@/store/slices/modelsSlice';

const isMac = navigator.userAgent.includes('Mac');

export default function SpeechToText() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const dispatch = useAppDispatch();
  const storeConfig = useAppSelector((s) => s.config.data);
  const models = useAppSelector((s) => s.models.whisperModels);
  const downloadProgress = useAppSelector((s) => s.models.downloadProgress);

  const [config, setConfig] = useState<Config | null>(null);
  const [devices, setDevices] = useState<string[]>([]);
  const [saved, setSaved] = useState(false);
  const [keyWarning, setKeyWarning] = useState<string | null>(null);
  const [hotkeyConflict, setHotkeyConflict] = useState<string | null>(null);
  const [openWindows, setOpenWindows] = useState<string[]>([]);
  const [modelPrompt, setModelPrompt] = useState<{ name: string; downloading: boolean } | null>(null);
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null);
  const [gpuBackend, setGpuBackend] = useState<string | null>(null);

  useEffect(() => {
    if (storeConfig) setConfig(structuredClone(storeConfig));
  }, [storeConfig]);

  useEffect(() => {
    api.listAudioDevices().then(setDevices).catch(() => {});
    api.listOpenWindows().then(setOpenWindows).catch(() => {});
    api.getSystemInfo().then(setSysInfo).catch(() => {});
    api.getDebugInfo().then((d) => setGpuBackend(d.gpu_backend)).catch(() => {});
  }, []);

  // Threads only matter for CPU inference; GPU builds offload the model entirely.
  const isGpuBuild = gpuBackend != null && gpuBackend !== 'cpu';

  useEffect(() => {
    if (!downloadProgress && modelPrompt?.downloading) {
      setModelPrompt(null);
    }
  }, [downloadProgress]);

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
      <Layout title={t('stt.title')} subtitle="Loading...">
        <div className="flex items-center justify-center py-20">
          <i className="ri-loader-4-line animate-spin text-slate-400 text-2xl" />
        </div>
      </Layout>
    );
  }

  return (
    <Layout title={t('stt.title')} subtitle={t('stt.subtitle')}>
      <div className="max-w-[860px] flex flex-col gap-5">
        {/* STT Backend */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.engine')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.engineDesc')}</p>

          <SettingRow label={t('stt.backend')} description={t('stt.backendDesc')}>
            <Select
              value={config.general.backend}
              onChange={(val) => {
                if (val === 'openai' && !config.openai.api_key) {
                  setKeyWarning('openai-stt');
                  return;
                }
                if (val === 'deepgram' && !config.deepgram.api_key) {
                  setKeyWarning('deepgram');
                  return;
                }
                if (val === 'smallest' && !config.smallest.api_key) {
                  setKeyWarning('smallest');
                  return;
                }
                setKeyWarning(null);
                update((c) => { c.general.backend = val; });
              }}
              options={[
                { value: 'whisper-local', label: t('stt.whisperLocal') },
                { value: 'openai', label: t('stt.openaiWhisper') },
                { value: 'deepgram', label: t('stt.deepgram') },
                { value: 'smallest', label: t('stt.smallest') },
              ]}
            />
          </SettingRow>

          {keyWarning === 'openai-stt' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mb-2">
              <i className="ri-key-line text-amber-500" />
              <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('stt.openaiKeyRequired')}</p>
              <button
                onClick={() => { setKeyWarning(null); navigate('/api-keys'); }}
                className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 underline cursor-pointer whitespace-nowrap"
              >
                {t('stt.addApiKey')}
              </button>
            </div>
          )}

          {keyWarning === 'deepgram' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mb-2">
              <i className="ri-key-line text-amber-500" />
              <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('stt.deepgramKeyRequired')}</p>
              <button
                onClick={() => { setKeyWarning(null); navigate('/api-keys'); }}
                className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 underline cursor-pointer whitespace-nowrap"
              >
                {t('stt.addApiKey')}
              </button>
            </div>
          )}

          {keyWarning === 'smallest' && (
            <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mb-2">
              <i className="ri-key-line text-amber-500" />
              <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('stt.smallestKeyRequired')}</p>
              <button
                onClick={() => { setKeyWarning(null); navigate('/api-keys'); }}
                className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 underline cursor-pointer whitespace-nowrap"
              >
                {t('stt.addApiKey')}
              </button>
            </div>
          )}

          {config.general.backend === 'openai' && (
            <SettingRow label={t('stt.openaiModel')} description={t('stt.openaiModelDesc')}>
              <Select
                value={config.openai.model}
                onChange={(val) => update((c) => { c.openai.model = val; })}
                options={[
                  { value: 'whisper-1', label: 'whisper-1' },
                ]}
              />
            </SettingRow>
          )}

          {config.general.backend === 'deepgram' && (
            <SettingRow label={t('stt.deepgramModel')} description={t('stt.deepgramModelDesc')}>
              <Select
                value={config.deepgram.model}
                onChange={(val) => update((c) => { c.deepgram.model = val; })}
                options={[
                  { value: 'nova-2', label: 'Nova-2' },
                  { value: 'nova-3', label: 'Nova-3' },
                ]}
              />
            </SettingRow>
          )}

          <SettingRow label={t('stt.language')} description={t('stt.languageDesc')}>
            <Select
              value={config.general.language}
              onChange={(val) => update((c) => { c.general.language = val; })}
              className="max-w-[200px]"
              options={[
                { value: 'auto', label: t('stt.autoDetect') },
                { value: 'en', label: 'en — English' },
                { value: 'zh', label: 'zh — Chinese' },
                { value: 'de', label: 'de — German' },
                { value: 'es', label: 'es — Spanish' },
                { value: 'ru', label: 'ru — Russian' },
                { value: 'ko', label: 'ko — Korean' },
                { value: 'fr', label: 'fr — French' },
                { value: 'ja', label: 'ja — Japanese' },
                { value: 'pt', label: 'pt — Portuguese' },
                { value: 'tr', label: 'tr — Turkish' },
                { value: 'pl', label: 'pl — Polish' },
                { value: 'ca', label: 'ca — Catalan' },
                { value: 'nl', label: 'nl — Dutch' },
                { value: 'ar', label: 'ar — Arabic' },
                { value: 'sv', label: 'sv — Swedish' },
                { value: 'it', label: 'it — Italian' },
                { value: 'id', label: 'id — Indonesian' },
                { value: 'hi', label: 'hi — Hindi' },
                { value: 'fi', label: 'fi — Finnish' },
                { value: 'vi', label: 'vi — Vietnamese' },
                { value: 'he', label: 'he — Hebrew' },
                { value: 'uk', label: 'uk — Ukrainian' },
                { value: 'el', label: 'el — Greek' },
                { value: 'ms', label: 'ms — Malay' },
                { value: 'cs', label: 'cs — Czech' },
                { value: 'ro', label: 'ro — Romanian' },
                { value: 'da', label: 'da — Danish' },
                { value: 'hu', label: 'hu — Hungarian' },
                { value: 'ta', label: 'ta — Tamil' },
                { value: 'no', label: 'no — Norwegian' },
                { value: 'th', label: 'th — Thai' },
                { value: 'ur', label: 'ur — Urdu' },
                { value: 'hr', label: 'hr — Croatian' },
                { value: 'bg', label: 'bg — Bulgarian' },
                { value: 'lt', label: 'lt — Lithuanian' },
                { value: 'la', label: 'la — Latin' },
                { value: 'mi', label: 'mi — Maori' },
                { value: 'ml', label: 'ml — Malayalam' },
                { value: 'cy', label: 'cy — Welsh' },
                { value: 'sk', label: 'sk — Slovak' },
                { value: 'te', label: 'te — Telugu' },
                { value: 'fa', label: 'fa — Persian' },
                { value: 'lv', label: 'lv — Latvian' },
                { value: 'bn', label: 'bn — Bengali' },
                { value: 'sr', label: 'sr — Serbian' },
                { value: 'az', label: 'az — Azerbaijani' },
                { value: 'sl', label: 'sl — Slovenian' },
                { value: 'kn', label: 'kn — Kannada' },
                { value: 'et', label: 'et — Estonian' },
                { value: 'mk', label: 'mk — Macedonian' },
                { value: 'br', label: 'br — Breton' },
                { value: 'eu', label: 'eu — Basque' },
                { value: 'is', label: 'is — Icelandic' },
                { value: 'hy', label: 'hy — Armenian' },
                { value: 'ne', label: 'ne — Nepali' },
                { value: 'ka', label: 'ka — Georgian' },
                { value: 'gl', label: 'gl — Galician' },
                { value: 'mr', label: 'mr — Marathi' },
                { value: 'tg', label: 'tg — Tajik' },
                { value: 'sw', label: 'sw — Swahili' },
                { value: 'oc', label: 'oc — Occitan' },
                { value: 'km', label: 'km — Khmer' },
                { value: 'sn', label: 'sn — Shona' },
                { value: 'yo', label: 'yo — Yoruba' },
                { value: 'so', label: 'so — Somali' },
                { value: 'af', label: 'af — Afrikaans' },
                { value: 'lb', label: 'lb — Luxembourgish' },
                { value: 'my', label: 'my — Myanmar' },
                { value: 'bo', label: 'bo — Tibetan' },
                { value: 'tl', label: 'tl — Tagalog' },
                { value: 'mg', label: 'mg — Malagasy' },
                { value: 'as', label: 'as — Assamese' },
                { value: 'tt', label: 'tt — Tatar' },
                { value: 'haw', label: 'haw — Hawaiian' },
                { value: 'ln', label: 'ln — Lingala' },
                { value: 'ha', label: 'ha — Hausa' },
                { value: 'ba', label: 'ba — Bashkir' },
                { value: 'jw', label: 'jw — Javanese' },
                { value: 'su', label: 'su — Sundanese' },
              ]}
            />
          </SettingRow>

          <SettingRow label={t('stt.clipboardOnly')} description={t('stt.clipboardOnlyDesc')}>
            <Toggle
              on={config.general.clipboard_only}
              onChange={(v) => update((c) => { c.general.clipboard_only = v; })}
            />
          </SettingRow>
        </div>

        {/* Push to Talk */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.pushToTalk')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.pushToTalkDesc')}</p>

          {config.general.hotkeys.map((hk, i) => (
            <div key={i} className="flex items-center gap-2 py-3 border-b border-slate-50 dark:border-slate-700 last:border-0">
              <div className="flex-1">
                <HotkeyButton
                  value={hk}
                  target="ptt"
                  onChange={(v) => {
                    if (config.hands_free.hotkeys.some((h) => hotkeysEqual(h, v))) {
                      setHotkeyConflict(t('stt.hotkeyConflictHandsFree', { hotkey: formatHotkey(v) }));
                      return;
                    }
                    update((c) => { c.general.hotkeys[i] = v; });
                  }}
                />
              </div>
              {config.general.hotkeys.length > 1 && (
                <button
                  onClick={() => update((c) => { c.general.hotkeys.splice(i, 1); })}
                  className="text-slate-400 hover:text-red-500 cursor-pointer p-1"
                  title="Remove hotkey"
                >
                  <i className="ri-delete-bin-line text-sm" />
                </button>
              )}
            </div>
          ))}

          {config.general.hotkeys.length < 5 && (
            <button
              onClick={() => update((c) => { c.general.hotkeys.push({ ...EMPTY_HOTKEY }); })}
              className="mt-3 text-xs text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 cursor-pointer flex items-center gap-1"
            >
              <i className="ri-add-line text-sm" />
              {t('stt.addHotkey')}
            </button>
          )}
        </div>

        {/* Hands-Free Mode */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.handsFree')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.handsFreeDesc')}</p>

          <SettingRow label={t('stt.handsFreeEnabled')} description={t('stt.handsFreeEnabledDesc')}>
            <Toggle
              on={config.hands_free.enabled}
              onChange={(v) => update((c) => { c.hands_free.enabled = v; })}
            />
          </SettingRow>

          {config.hands_free.enabled && (
            <>
              {config.hands_free.hotkeys.map((hk, i) => (
                <div key={i} className="flex items-center gap-2 py-3 border-b border-slate-50 dark:border-slate-700 last:border-0">
                  <div className="flex-1">
                    <HotkeyButton
                      value={hk}
                      target="handsfree"
                      onChange={(v) => {
                        if (config.general.hotkeys.some((h) => hotkeysEqual(h, v))) {
                          setHotkeyConflict(t('stt.hotkeyConflictPushToTalk', { hotkey: formatHotkey(v) }));
                          return;
                        }
                        update((c) => { c.hands_free.hotkeys[i] = v; });
                      }}
                    />
                  </div>
                  {config.hands_free.hotkeys.length > 1 && (
                    <button
                      onClick={() => update((c) => { c.hands_free.hotkeys.splice(i, 1); })}
                      className="text-slate-400 hover:text-red-500 cursor-pointer p-1"
                      title="Remove hotkey"
                    >
                      <i className="ri-delete-bin-line text-sm" />
                    </button>
                  )}
                </div>
              ))}

              {config.hands_free.hotkeys.length < 5 && (
                <button
                  onClick={() => update((c) => { c.hands_free.hotkeys.push({ ...EMPTY_HOTKEY }); })}
                  className="mt-3 text-xs text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 cursor-pointer flex items-center gap-1"
                >
                  <i className="ri-add-line text-sm" />
                  {t('stt.addHotkey')}
                </button>
              )}

              {config.hands_free.hotkeys.length === 0 && (
                <div className="flex items-center gap-3 py-3 px-4 bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/30 rounded-lg mt-3">
                  <i className="ri-information-line text-amber-500" />
                  <p className="text-amber-700 dark:text-amber-400 text-xs flex-1">{t('stt.handsFreeNoHotkeys')}</p>
                </div>
              )}
            </>
          )}
        </div>

        {/* Audio */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.audio')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.audioDesc')}</p>

          <SettingRow label={t('stt.noiseCancellation')} description={t('stt.noiseCancellationDesc')}>
            <Toggle
              on={config.audio.noise_cancellation}
              onChange={(v) => update((c) => { c.audio.noise_cancellation = v; })}
            />
          </SettingRow>

          <SettingRow label={t('stt.inputDevice')} description={t('stt.inputDeviceDesc')}>
            <Select
              value={config.audio.device}
              onChange={(val) => update((c) => { c.audio.device = val; })}
              className="max-w-[280px]"
              options={[
                { value: '', label: t('common.default') },
                ...devices.map((d) => ({ value: d, label: d })),
              ]}
            />
          </SettingRow>

          {!isMac && (
            <SettingRow label={t('stt.inputMethod')} description={t('stt.inputMethodDesc')}>
              <Select
                value={config.input.method}
                onChange={(val) => update((c) => { c.input.method = val; })}
                options={[
                  { value: 'auto', label: t('stt.inputMethodAuto') },
                  { value: 'enigo', label: t('stt.inputMethodEnigo') },
                  { value: 'wtype', label: t('stt.inputMethodWtype') },
                ]}
              />
            </SettingRow>
          )}

          <SettingRow label={t('stt.defaultPasteCommand')} description={t('stt.defaultPasteCommandDesc')}>
            <PasteKeyButton
              value={config.input.paste_command}
              onChange={(v) => update((c) => { c.input.paste_command = v; })}
            />
          </SettingRow>

          <div className="py-4 border-b border-slate-50 dark:border-slate-700 last:border-0">
            <div className="flex items-center justify-between mb-3">
              <div>
                <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{t('stt.perAppPaste')}</p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('stt.perAppPasteDesc')}</p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => api.listOpenWindows().then(setOpenWindows).catch(() => {})}
                  className="text-xs text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 cursor-pointer"
                  title={t('stt.refreshWindows')}
                >
                  <i className="ri-refresh-line text-sm" />
                </button>
                <Select
                  value=""
                  onChange={(appClass) => {
                    if (!appClass) return;
                    const alreadyExists = config.input.paste_rules.some(
                      (r: PasteRule) => r.app_class.toLowerCase() === appClass.toLowerCase()
                    );
                    if (!alreadyExists) {
                      update((c) => {
                        c.input.paste_rules.push({ app_class: appClass, paste_command: 'ctrl+shift+v' });
                      });
                    }
                  }}
                  placeholder={t('stt.addApp')}
                  options={openWindows
                    .filter((w) => !config.input.paste_rules.some(
                      (r: PasteRule) => r.app_class.toLowerCase() === w.toLowerCase()
                    ))
                    .map((w) => ({ value: w, label: w }))
                  }
                />
              </div>
            </div>
            {config.input.paste_rules.map((rule: PasteRule, i: number) => (
              <div key={i} className="flex items-center gap-2 mb-2">
                <span className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 w-40 font-mono truncate" title={rule.app_class}>
                  {rule.app_class}
                </span>
                <PasteKeyButton
                  value={rule.paste_command}
                  onChange={(v) => update((c) => { c.input.paste_rules[i].paste_command = v; })}
                />
                <button
                  onClick={() => update((c) => { c.input.paste_rules.splice(i, 1); })}
                  className="text-slate-400 hover:text-red-500 cursor-pointer p-1"
                  title="Remove rule"
                >
                  <i className="ri-delete-bin-line text-sm" />
                </button>
              </div>
            ))}
            {config.input.paste_rules.length === 0 && (
              <p className="text-slate-300 dark:text-slate-600 text-xs italic">{t('stt.noPerAppRules')}</p>
            )}
          </div>
        </div>

        {/* Whisper Local */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.whisperLocalTitle')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.whisperLocalDesc')}</p>

          <SettingRow label={t('stt.model')} description={t('stt.modelDesc')}>
            <Select
              value={config.whisper.model}
              onChange={(val) => {
                const m = models.find((m) => m.name === val);
                if (m && !m.downloaded) {
                  setModelPrompt({ name: val, downloading: false });
                  return;
                }
                update((c) => { c.whisper.model = val; });
              }}
              className="w-36 font-mono"
              options={[
                'tiny',
                'tiny.en',
                'base',
                'base.en',
                'small',
                'small.en',
                'medium',
                'medium.en',
                'large-v3',
              ].map((name) => {
                const m = models.find((x) => x.name === name);
                const downloaded = !!m?.downloaded;
                return {
                  value: name,
                  label: name,
                  hint: downloaded ? undefined : '(not downloaded)',
                  disabled: !downloaded,
                };
              })}
            />
          </SettingRow>

          <SettingRow label={t('stt.threads')} description={t('stt.threadsDesc')}>
            <input
              type="number"
              value={config.whisper.threads}
              onChange={(e) => update((c) => { c.whisper.threads = parseInt(e.target.value) || 0; })}
              disabled={isGpuBuild}
              title={isGpuBuild ? t('stt.threadsGpuDisabled', { backend: gpuBackend?.toUpperCase() }) : undefined}
              className="text-sm text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-2 outline-none w-20 disabled:opacity-50 disabled:cursor-not-allowed"
              min={0}
            />
          </SettingRow>
        </div>

        {/* STT Models */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.sttModels')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('stt.sttModelsDesc')}</p>

          <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
            {models.map((m) => {
              const isDownloading = downloadProgress?.model === m.name;
              const pct = isDownloading && downloadProgress.total > 0
                ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100) : 0;
              const sizeMb = (m.size_bytes / 1_000_000).toFixed(0);
              const meta = WHISPER_META[m.name];
              const realtime = sysInfo ? estimateWhisperRealtime(m.name, sysInfo) : null;
              // Realtime-factor-driven tier for STT. Anything under ~3× is
              // sluggish for live dictation; under 1× can't keep up.
              const score = sysInfo && meta ? (() => {
                const base = scoreCompatibility({
                  total_ram_mb: sysInfo.total_ram_mb,
                  cpu_cores: sysInfo.cpu_cores,
                  platform: sysInfo.platform,
                  min_ram_mb: meta.working_set_mb,
                  recommended_ram_mb: meta.working_set_mb,
                  recommended_cores: meta.recommended_cores,
                });
                if (realtime == null) return base;
                let tier = base.tier;
                if (realtime < 1) tier = 'too_large';
                else if (realtime < 3 && (tier === 'best' || tier === 'fits')) tier = 'tight';
                else if (realtime < 10 && tier === 'best') tier = 'fits';
                return { ...base, tier, reason: realtime < 10 && base.tier === 'best' ? 'throughput' as const : base.reason };
              })() : null;
              const badgeStyles: Record<Tier, string> = {
                best:      'text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-500/10 border-emerald-200 dark:border-emerald-500/30',
                fits:      'text-emerald-700 dark:text-emerald-300 bg-emerald-50 dark:bg-emerald-500/10 border-emerald-200 dark:border-emerald-500/30',
                tight:     'text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30',
                too_large: 'text-rose-700 dark:text-rose-300 bg-rose-50 dark:bg-rose-500/10 border-rose-200 dark:border-rose-500/30',
              };
              const badgeLabels: Record<Tier, string> = {
                best:      t('stt.suitBest'),
                fits:      t('stt.suitFits'),
                tight:     t('stt.suitTight'),
                too_large: t('stt.suitTooLarge'),
              };
              return (
                <div key={m.name} className="flex items-center justify-between py-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <span className="text-slate-800 dark:text-slate-200 text-sm font-medium font-mono">{m.name}</span>
                    <span className="text-slate-400 dark:text-slate-500 text-xs">{sizeMb} MB</span>
                    {score && (
                      <span
                        className={`px-1.5 py-0.5 rounded text-[10px] font-medium border ${badgeStyles[score.tier]}`}
                        title={score.reason === 'cpu' ? t('pp.ollamaWarnSlowCpu', { cores: sysInfo!.cpu_cores }) : undefined}
                      >
                        {badgeLabels[score.tier]}
                      </span>
                    )}
                    {realtime != null && (
                      <span className="text-slate-400 dark:text-slate-500 text-[10px] font-mono tabular-nums">
                        {fmtRealtime(realtime)}
                      </span>
                    )}
                    {m.downloaded && <span className="text-emerald-500 text-xs font-medium flex items-center gap-1"><i className="ri-check-line text-xs" />{t('common.downloaded')}</span>}
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
                        >{t('common.cancel')}</button>
                      </>
                    ) : m.downloaded ? (
                      <button
                        onClick={() => api.deleteModel(m.name).then(() => dispatch(fetchWhisperModels()))}
                        className="px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all"
                      >{t('common.delete')}</button>
                    ) : (
                      <button
                        onClick={() => api.downloadModel(m.name)}
                        disabled={downloadProgress !== null}
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

      {/* Hotkey conflict modal */}
      {hotkeyConflict && (
        <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6" onClick={() => setHotkeyConflict(null)}>
          <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-sm" onClick={(e) => e.stopPropagation()}>
            <div className="p-6 text-center">
              <div className="w-10 h-10 rounded-full bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center mx-auto mb-3">
                <i className="ri-error-warning-line text-amber-500 text-xl" />
              </div>
              <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.keyNotAllowed')}</h3>
              <p className="text-slate-500 dark:text-slate-400 text-xs">{hotkeyConflict}</p>
            </div>
            <div className="px-6 pb-5 flex justify-center">
              <button onClick={() => setHotkeyConflict(null)} className="px-4 py-2 text-xs font-medium text-white bg-amber-500 hover:bg-amber-600 rounded-lg cursor-pointer">OK</button>
            </div>
          </div>
        </div>
      )}

      {/* Whisper model download modal */}
      {modelPrompt && (
        <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6">
          <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-sm" onClick={(e) => e.stopPropagation()}>
            <div className="p-6 text-center">
              <div className="w-10 h-10 rounded-full bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center mx-auto mb-3">
                <i className="ri-download-line text-amber-500 text-xl" />
              </div>
              <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('stt.modelNotDownloaded')}</h3>
              <p className="text-slate-500 dark:text-slate-400 text-xs">
                {t('stt.modelNotDownloadedDesc', { model: modelPrompt.name })}
              </p>
              {modelPrompt.downloading && downloadProgress && (
                <div className="mt-4 flex items-center gap-3">
                  <div className="flex-1 bg-slate-100 dark:bg-slate-700 rounded-full h-2 overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-all ${downloadProgress.verifying ? 'bg-sky-400 animate-pulse' : 'bg-amber-400'}`}
                      style={{ width: downloadProgress.verifying ? '100%' : `${downloadProgress.total > 0 ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100) : 0}%` }}
                    />
                  </div>
                  <span className={`text-xs tabular-nums whitespace-nowrap ${downloadProgress.verifying ? 'text-sky-500' : 'text-slate-500 dark:text-slate-400 w-10 text-right'}`}>
                    {downloadProgress.verifying ? t('common.verifying') : `${downloadProgress.total > 0 ? Math.round((downloadProgress.downloaded / downloadProgress.total) * 100) : 0}%`}
                  </span>
                </div>
              )}
            </div>
            <div className="px-6 pb-5 flex justify-center gap-3">
              {modelPrompt.downloading ? (
                <button onClick={() => api.cancelModelDownload()} className="px-4 py-2 text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-lg cursor-pointer transition-all">{t('common.cancel')}</button>
              ) : (
                <>
                  <button onClick={() => setModelPrompt(null)} className="px-4 py-2 text-xs font-medium text-slate-600 dark:text-slate-400 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 border border-slate-200 dark:border-slate-600 rounded-lg cursor-pointer transition-all">{t('common.cancel')}</button>
                  <button
                    onClick={() => {
                      setModelPrompt({ ...modelPrompt, downloading: true });
                      update((c) => { c.whisper.model = modelPrompt.name; });
                      api.downloadModel(modelPrompt.name);
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
