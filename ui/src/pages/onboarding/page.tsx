import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { api } from '@/lib/tauri';
import type { MacPermissions } from '@/lib/types';

const isMac = navigator.userAgent.includes('Mac');
const isLinux = !isMac && navigator.userAgent.includes('Linux');

const LINUX_INPUT_GROUP_CMD = 'sudo usermod -aG input $USER && newgrp input';

type RowKey = 'accessibility' | 'microphone' | 'input-monitoring' | 'automation' | 'linux-input';

interface Row {
  key: RowKey;
  titleKey: string;
  descKey: string;
  icon: string;
  granted: boolean;
  /// Required permissions block "Continue". Optional ones don't.
  required: boolean;
  /// macOS pane for openMacSettings; null for Linux row (uses copy-paste cmd).
  pane: string | null;
}

export default function Onboarding() {
  const navigate = useNavigate();
  const { t } = useTranslation();

  const [perms, setPerms] = useState<MacPermissions | null>(null);
  const [linuxInputOk, setLinuxInputOk] = useState<boolean>(true);
  const [checking, setChecking] = useState(false);
  const [copied, setCopied] = useState(false);

  const refresh = useCallback(async () => {
    setChecking(true);
    try {
      const [mac, linux] = await Promise.all([
        api.checkMacPermissions(),
        api.checkLinuxInputPermission(),
      ]);
      setPerms(mac); // null on non-macOS — checklist hides the macOS rows
      setLinuxInputOk(linux);
    } catch {
      // Leave previous values; user can retry.
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const finish = useCallback(async () => {
    try {
      const config = await api.getConfig();
      if (!config.general.onboarding_complete) {
        config.general.onboarding_complete = true;
        await api.saveConfig(config);
      }
    } catch {
      /* non-fatal */
    }
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
    } catch {
      /* clipboard might be blocked in some contexts */
    }
  }, []);

  const rows: Row[] = [];
  if (isMac && perms) {
    rows.push(
      {
        key: 'accessibility',
        titleKey: 'onboarding.accessibility',
        descKey: 'onboarding.accessibilityDesc',
        icon: 'ri-keyboard-line',
        granted: perms.accessibility,
        required: true,
        pane: 'accessibility',
      },
      {
        key: 'microphone',
        titleKey: 'onboarding.microphone',
        descKey: 'onboarding.microphoneDesc',
        icon: 'ri-mic-line',
        granted: perms.microphone,
        required: true,
        pane: 'microphone',
      },
      {
        key: 'input-monitoring',
        titleKey: 'onboarding.inputMonitoring',
        descKey: 'onboarding.inputMonitoringDesc',
        icon: 'ri-radar-line',
        granted: perms.input_monitoring,
        required: true,
        pane: 'input-monitoring',
      },
      {
        key: 'automation',
        titleKey: 'onboarding.automation',
        descKey: 'onboarding.automationDesc',
        icon: 'ri-terminal-window-line',
        granted: perms.automation,
        required: false,
        pane: 'automation',
      },
    );
  }
  if (isLinux) {
    rows.push({
      key: 'linux-input',
      titleKey: 'onboarding.linuxInput',
      descKey: 'onboarding.linuxInputDesc',
      icon: 'ri-keyboard-line',
      granted: linuxInputOk,
      required: true,
      pane: null,
    });
  }

  const requiredOk = rows.filter((r) => r.required).every((r) => r.granted);
  const allOk = rows.every((r) => r.granted);

  return (
    <div
      className="flex h-screen items-center justify-center bg-[#f8f9fa] dark:bg-slate-900 overflow-y-auto"
      style={{ fontFamily: "'Inter', sans-serif" }}
    >
      <div className="w-full max-w-lg mx-auto px-6 py-10">
        <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 p-8 shadow-sm">
          <div className="w-14 h-14 bg-amber-50 dark:bg-amber-500/10 rounded-2xl flex items-center justify-center mb-5">
            <i
              className={`${
                allOk ? 'ri-check-double-line text-emerald-500' : 'ri-shield-check-line text-amber-500'
              } text-2xl`}
            />
          </div>

          <h1 className="text-lg font-semibold text-slate-900 dark:text-slate-100 mb-1">
            {allOk ? t('onboarding.allSet') : t('onboarding.checklistTitle')}
          </h1>
          <p className="text-sm text-slate-500 dark:text-slate-400 mb-6 leading-relaxed">
            {allOk ? t('onboarding.allSetDesc') : t('onboarding.checklistDesc')}
          </p>

          {rows.length === 0 && (
            <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">
              {t('onboarding.allSetDesc')}
            </p>
          )}

          <div className="flex flex-col gap-3 mb-6">
            {rows.map((row) => (
              <div
                key={row.key}
                className={`rounded-xl border p-4 transition-all ${
                  row.granted
                    ? 'bg-emerald-50/60 dark:bg-emerald-500/5 border-emerald-200 dark:border-emerald-500/20'
                    : 'bg-slate-50 dark:bg-slate-900/40 border-slate-200 dark:border-slate-700'
                }`}
              >
                <div className="flex items-start gap-3 mb-2">
                  <div
                    className={`w-9 h-9 rounded-lg flex items-center justify-center shrink-0 ${
                      row.granted
                        ? 'bg-emerald-100 dark:bg-emerald-500/20'
                        : 'bg-slate-100 dark:bg-slate-700'
                    }`}
                  >
                    <i
                      className={`${row.icon} text-lg ${
                        row.granted
                          ? 'text-emerald-600 dark:text-emerald-400'
                          : 'text-slate-500 dark:text-slate-400'
                      }`}
                    />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">
                        {t(row.titleKey)}
                      </h3>
                      {!row.required && (
                        <span className="text-[10px] font-medium uppercase tracking-wide px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-700 text-slate-500 dark:text-slate-400">
                          {t('onboarding.optional')}
                        </span>
                      )}
                      {row.granted ? (
                        <span className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                          <i className="ri-checkbox-circle-fill" />
                          {t('onboarding.granted')}
                        </span>
                      ) : (
                        <span className="ml-auto inline-flex items-center gap-1 text-xs font-medium text-slate-500 dark:text-slate-400">
                          <i className="ri-close-circle-line" />
                          {t('onboarding.notGranted')}
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-slate-500 dark:text-slate-400 mt-1 leading-relaxed">
                      {t(row.descKey)}
                    </p>
                  </div>
                </div>

                {!row.granted && row.pane && (
                  <button
                    onClick={() => openSettings(row.pane!)}
                    className="w-full flex items-center justify-center gap-2 px-3 py-2 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-lg font-medium text-xs transition-all"
                  >
                    <i className="ri-external-link-line" />
                    {t('onboarding.openSettings')}
                  </button>
                )}

                {!row.granted && row.key === 'linux-input' && (
                  <div className="space-y-2">
                    <div className="text-xs text-slate-500 dark:text-slate-400">
                      {t('onboarding.linuxInputCommand')}
                    </div>
                    <div className="flex items-center gap-2">
                      <code className="flex-1 px-2 py-1.5 rounded bg-slate-900 text-slate-100 text-[11px] font-mono overflow-x-auto whitespace-nowrap">
                        {LINUX_INPUT_GROUP_CMD}
                      </code>
                      <button
                        onClick={copyLinuxCmd}
                        className="px-2 py-1.5 rounded bg-slate-100 dark:bg-slate-700 hover:bg-slate-200 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 text-xs font-medium"
                      >
                        <i className={copied ? 'ri-check-line' : 'ri-clipboard-line'} />
                      </button>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>

          <div className="flex flex-col gap-2">
            <button
              onClick={refresh}
              disabled={checking}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-xl font-medium text-sm transition-all disabled:opacity-50"
            >
              {checking ? (
                <>
                  <i className="ri-loader-4-line animate-spin" />
                  {t('onboarding.checkPermission')}
                </>
              ) : (
                <>
                  <i className="ri-refresh-line" />
                  {t('onboarding.checkPermission')}
                </>
              )}
            </button>
            <button
              onClick={finish}
              disabled={!requiredOk}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-amber-500 hover:bg-amber-600 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-xl font-semibold text-sm transition-all"
            >
              {requiredOk ? t('onboarding.continue') : t('onboarding.continueBlocked')}
            </button>
          </div>
        </div>

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
