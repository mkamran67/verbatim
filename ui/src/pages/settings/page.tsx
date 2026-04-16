import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import type { Config } from '@/lib/types';
import { SettingRow, Toggle } from './components/SettingRow';
import DebugSection from './components/DebugSection';
import Select from '@/components/ui/Select';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { themeChanged } from '@/store/slices/themeSlice';
import type { Theme } from '@/lib/theme';

const themeOptions = [
  { value: 'light', labelKey: 'settings.light', icon: 'ri-sun-line' },
  { value: 'dark', labelKey: 'settings.dark', icon: 'ri-moon-line' },
  { value: 'system', labelKey: 'settings.system', icon: 'ri-computer-line' },
] as const;

const languageOptions = [
  { value: 'system', labelKey: 'settings.languageSystem' },
  { value: 'en', labelKey: 'settings.languageEn' },
  { value: 'es', labelKey: 'settings.languageEs' },
  { value: 'de', labelKey: 'settings.languageDe' },
  { value: 'fr', labelKey: 'settings.languageFr' },
  { value: 'ja', labelKey: 'settings.languageJa' },
] as const;

export default function Settings() {
  const { t, i18n } = useTranslation();
  const dispatch = useAppDispatch();
  const theme = useAppSelector((s) => s.theme.value);
  const storeConfig = useAppSelector((s) => s.config.data);

  const [config, setConfig] = useState<Config | null>(null);
  const [saved, setSaved] = useState(false);
  const [micTesting, setMicTesting] = useState(false);
  const [micLevel, setMicLevel] = useState(0);

  useEffect(() => {
    if (storeConfig && !config) setConfig(structuredClone(storeConfig));
  }, [storeConfig]);

  useEffect(() => {
    if (!micTesting) return;
    const interval = setInterval(() => {
      api.getMicLevel().then(setMicLevel).catch(() => {});
    }, 60);
    return () => clearInterval(interval);
  }, [micTesting]);

  useEffect(() => {
    return () => { api.stopMicMonitor().catch(() => {}); };
  }, []);

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
      <Layout title={t('settings.title')} subtitle={t('common.loading')}>
        <div className="flex items-center justify-center py-20">
          <i className="ri-loader-4-line animate-spin text-slate-400 text-2xl" />
        </div>
      </Layout>
    );
  }

  return (
    <Layout title={t('settings.title')} subtitle={t('settings.subtitle')}>
      <div className="max-w-[860px] flex flex-col gap-5">
        {/* Appearance */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('settings.appearance')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('settings.appearanceDesc')}</p>

          <SettingRow label={t('settings.theme')} description={t('settings.themeDesc')}>
            <div className="flex items-center bg-slate-100 dark:bg-slate-700 rounded-lg p-0.5">
              {themeOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => {
                    dispatch(themeChanged(opt.value as Theme));
                    update((c) => { c.general.theme = opt.value; });
                  }}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium transition-all cursor-pointer ${
                    theme === opt.value
                      ? 'bg-white dark:bg-slate-600 text-slate-900 dark:text-slate-100 shadow-sm'
                      : 'text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-300'
                  }`}
                >
                  <i className={`${opt.icon} text-sm`} />
                  {t(opt.labelKey)}
                </button>
              ))}
            </div>
          </SettingRow>

          <SettingRow label={t('settings.language')} description={t('settings.languageDesc')}>
            <Select
              value={config.general.ui_language || 'system'}
              onChange={(val) => {
                update((c) => { c.general.ui_language = val; });
                if (val === 'system') {
                  // Let the browser language detector handle it
                  const detected = navigator.language.split('-')[0];
                  const supported = ['en', 'es', 'de', 'fr', 'ja'];
                  i18n.changeLanguage(supported.includes(detected) ? detected : 'en');
                } else {
                  i18n.changeLanguage(val);
                }
              }}
              options={languageOptions.map((opt) => ({ value: opt.value, label: t(opt.labelKey) }))}
            />
          </SettingRow>
        </div>

        {/* Mic Test */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-sm mb-1">{t('settings.micTest')}</h3>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-5">{t('settings.micTestDesc')}</p>

          <SettingRow label={t('stt.noiseCancellation')} description={t('stt.noiseCancellationDesc')}>
            <Toggle
              on={config.audio.noise_cancellation}
              onChange={(v) => update((c) => { c.audio.noise_cancellation = v; })}
            />
          </SettingRow>

          <SettingRow label={t('settings.testMicrophone')} description={t('settings.testMicrophoneDesc')}>
            <button
              onClick={async () => {
                if (micTesting) {
                  setMicTesting(false);
                  setMicLevel(0);
                  await api.stopMicMonitor();
                } else {
                  await api.startMicMonitor();
                  setMicTesting(true);
                }
              }}
              className={`px-4 py-1.5 rounded-lg text-xs font-medium cursor-pointer transition-all ${
                micTesting
                  ? 'bg-red-500 hover:bg-red-600 text-white'
                  : 'bg-amber-500 hover:bg-amber-600 text-white'
              }`}
            >
              {micTesting ? t('settings.stopTest') : t('settings.startTest')}
            </button>
          </SettingRow>

          <div className="py-4 border-b border-slate-50 dark:border-slate-700">
            <p className="text-slate-800 dark:text-slate-200 text-sm font-medium mb-2">{t('settings.levelMeter')}</p>
            <div className="relative h-6 bg-slate-100 dark:bg-slate-700 rounded-full overflow-hidden">
              <div
                className="absolute inset-y-0 left-0 bg-gradient-to-r from-emerald-400 via-yellow-400 to-red-500 rounded-full transition-all duration-75"
                style={{ width: `${Math.min(micLevel * 500, 100)}%` }}
              />
              {config.audio.energy_threshold > 0 && (
                <div
                  className="absolute inset-y-0 w-0.5 bg-red-600 dark:bg-red-400 z-10"
                  style={{ left: `${Math.min(config.audio.energy_threshold * 500, 100)}%` }}
                />
              )}
            </div>
            <div className="flex justify-between mt-1">
              <span className="text-slate-400 text-[10px]">0</span>
              <span className="text-slate-400 text-[10px] font-mono">RMS: {micLevel.toFixed(4)}</span>
              <span className="text-slate-400 text-[10px]">0.2</span>
            </div>
          </div>

          <SettingRow label={t('settings.energyThreshold')} description={t('settings.energyThresholdDesc')}>
            <div className="flex items-center gap-3">
              <input
                type="range"
                min="0"
                max="0.2"
                step="0.001"
                value={config.audio.energy_threshold}
                onChange={(e) => update((c) => { c.audio.energy_threshold = parseFloat(e.target.value); })}
                className="w-32 accent-amber-500"
              />
              <span className="text-xs font-mono text-slate-500 dark:text-slate-400 w-12 text-right">
                {config.audio.energy_threshold.toFixed(3)}
              </span>
            </div>
          </SettingRow>
        </div>

        {/* Debug */}
        <DebugSection />

        {saved && (
          <div className="flex items-center justify-end">
            <span className="text-emerald-500 text-xs font-medium">{t('common.saved')}</span>
          </div>
        )}
      </div>
    </Layout>
  );
}
