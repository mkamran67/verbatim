import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { api } from '@/lib/tauri';
import type { MacPermissions } from '@/lib/types';

type Step = 'accessibility' | 'microphone' | 'done';

const STEPS: { key: Step; titleKey: string; icon: string; descriptionKey: string; pane: string }[] = [
  {
    key: 'accessibility',
    titleKey: 'onboarding.accessibility',
    icon: 'ri-keyboard-line',
    descriptionKey: 'onboarding.accessibilityDesc',
    pane: 'accessibility',
  },
  {
    key: 'microphone',
    titleKey: 'onboarding.microphone',
    icon: 'ri-mic-line',
    descriptionKey: 'onboarding.microphoneDesc',
    pane: 'microphone',
  },
];

export default function Onboarding() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [currentStep, setCurrentStep] = useState<Step>('accessibility');
  const [permissions, setPermissions] = useState<MacPermissions>({ accessibility: false, microphone: false });
  const [checking, setChecking] = useState(false);
  const [denied, setDenied] = useState(false);

  const checkPermissions = useCallback(async () => {
    setChecking(true);
    setDenied(false);
    try {
      const result = await api.checkMacPermissions();
      if (!result) {
        // Not macOS, skip onboarding
        navigate('/', { replace: true });
        return;
      }
      setPermissions(result);

      if (currentStep === 'accessibility') {
        if (result.accessibility) {
          setCurrentStep(result.microphone ? 'done' : 'microphone');
        } else {
          setDenied(true);
        }
      } else if (currentStep === 'microphone') {
        if (result.microphone) {
          setCurrentStep('done');
        } else {
          setDenied(true);
        }
      }
    } catch {
      // If the command fails, skip onboarding
      navigate('/', { replace: true });
    } finally {
      setChecking(false);
    }
  }, [currentStep, navigate]);

  const finish = useCallback(async () => {
    try {
      const config = await api.getConfig();
      config.general.onboarding_complete = true;
      await api.saveConfig(config);
    } catch {
      // Continue even if save fails
    }
    navigate('/', { replace: true });
  }, [navigate]);

  const openSettings = useCallback((pane: string) => {
    api.openMacSettings(pane).catch(() => {});
  }, []);

  // Done state
  if (currentStep === 'done') {
    return (
      <div className="flex h-screen items-center justify-center bg-[#f8f9fa] dark:bg-slate-900" style={{ fontFamily: "'Inter', sans-serif" }}>
        <div className="w-full max-w-md mx-auto px-6">
          <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 p-8 text-center shadow-sm">
            <div className="w-16 h-16 bg-emerald-50 dark:bg-emerald-500/10 rounded-2xl flex items-center justify-center mx-auto mb-5">
              <i className="ri-check-double-line text-3xl text-emerald-500" />
            </div>
            <h1 className="text-xl font-semibold text-slate-900 dark:text-slate-100 mb-2">
              {t('onboarding.allSet')}
            </h1>
            <p className="text-sm text-slate-500 dark:text-slate-400 mb-6">
              {t('onboarding.allSetDesc')}
            </p>
            <div className="flex flex-col gap-2 mb-6">
              {STEPS.map((s) => (
                <div key={s.key} className="flex items-center gap-3 px-4 py-2 rounded-lg bg-emerald-50 dark:bg-emerald-500/10">
                  <i className="ri-checkbox-circle-fill text-emerald-500" />
                  <span className="text-sm font-medium text-emerald-700 dark:text-emerald-400">{t(s.titleKey)}</span>
                </div>
              ))}
            </div>
            <button
              onClick={finish}
              className="w-full px-6 py-3 bg-amber-500 hover:bg-amber-600 text-white rounded-xl font-semibold text-sm transition-all"
            >
              {t('onboarding.getStarted')}
            </button>
          </div>
        </div>
      </div>
    );
  }

  const step = STEPS.find((s) => s.key === currentStep)!;
  const stepIndex = STEPS.findIndex((s) => s.key === currentStep);

  return (
    <div className="flex h-screen items-center justify-center bg-[#f8f9fa] dark:bg-slate-900" style={{ fontFamily: "'Inter', sans-serif" }}>
      <div className="w-full max-w-md mx-auto px-6">
        {/* Progress */}
        <div className="flex items-center justify-center gap-2 mb-6">
          {STEPS.map((s, i) => (
            <div key={s.key} className="flex items-center gap-2">
              <div
                className={`w-8 h-8 rounded-full flex items-center justify-center text-xs font-semibold transition-all ${
                  i < stepIndex
                    ? 'bg-emerald-500 text-white'
                    : i === stepIndex
                    ? 'bg-amber-500 text-white'
                    : 'bg-slate-200 dark:bg-slate-700 text-slate-400 dark:text-slate-500'
                }`}
              >
                {i < stepIndex ? <i className="ri-check-line" /> : i + 1}
              </div>
              {i < STEPS.length - 1 && (
                <div className={`w-12 h-0.5 ${i < stepIndex ? 'bg-emerald-500' : 'bg-slate-200 dark:bg-slate-700'}`} />
              )}
            </div>
          ))}
        </div>

        {/* Card */}
        <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 p-8 shadow-sm">
          <div className="w-14 h-14 bg-amber-50 dark:bg-amber-500/10 rounded-2xl flex items-center justify-center mb-5">
            <i className={`${step.icon} text-2xl text-amber-500`} />
          </div>

          <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-2">
            {t(step.titleKey)} {t('onboarding.permission')}
          </h1>
          <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">
            {t(step.descriptionKey)}
          </p>

          {denied && (
            <div className="flex items-start gap-2 px-4 py-3 rounded-lg bg-red-50 dark:bg-red-500/10 border border-red-100 dark:border-red-500/20 mb-4">
              <i className="ri-error-warning-line text-red-500 mt-0.5" />
              <p className="text-xs text-red-600 dark:text-red-400">
                {t('onboarding.permissionDenied')}
              </p>
            </div>
          )}

          <div className="flex flex-col gap-3">
            <button
              onClick={() => openSettings(step.pane)}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-xl font-medium text-sm transition-all"
            >
              <i className="ri-external-link-line" />
              {t('onboarding.openSettings')}
            </button>
            <button
              onClick={checkPermissions}
              disabled={checking}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 text-white rounded-xl font-semibold text-sm transition-all"
            >
              {checking ? (
                <>
                  <i className="ri-loader-4-line animate-spin" />
                  {t('onboarding.checkPermission')}...
                </>
              ) : (
                <>
                  <i className="ri-shield-check-line" />
                  {t('onboarding.checkPermission')}
                </>
              )}
            </button>
          </div>
        </div>

        {/* Skip link */}
        <div className="text-center mt-4">
          <button
            onClick={finish}
            className="text-xs text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-400 transition-colors"
          >
            {t('onboarding.skipForNow')}
          </button>
        </div>
      </div>
    </div>
  );
}
