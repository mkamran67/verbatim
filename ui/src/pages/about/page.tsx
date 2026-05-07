import { useState, useEffect } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import type { UpdateInfo } from '@/lib/types';

function Section({ title, icon, children, defaultOpen = false }: { title: string; icon: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700">
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-3 px-5 py-4 text-left cursor-pointer"
      >
        <div className="w-8 h-8 rounded-lg bg-amber-50 dark:bg-amber-500/10 flex items-center justify-center flex-shrink-0">
          <i className={`${icon} text-amber-500 text-base`} />
        </div>
        <span className="text-slate-800 dark:text-slate-200 text-sm font-semibold flex-1">{title}</span>
        <i className={`ri-arrow-down-s-line text-slate-400 text-lg transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>
      {open && (
        <div className="px-5 pb-5 text-slate-600 dark:text-slate-400 text-xs leading-relaxed space-y-3">
          <div className="border-t border-slate-100 dark:border-slate-700 pt-4">
            {children}
          </div>
        </div>
      )}
    </div>
  );
}

export default function About() {
  const { t } = useTranslation();
  const [version, setVersion] = useState('');
  const [updateState, setUpdateState] = useState<'idle' | 'checking' | 'done' | 'error'>('idle');
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateError, setUpdateError] = useState('');

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion('0.1.0'));
  }, []);

  const checkForUpdate = async () => {
    setUpdateState('checking');
    setUpdateError('');
    try {
      const info = await api.checkForUpdate();
      setUpdateInfo(info);
      setUpdateState('done');
    } catch (e) {
      setUpdateError(e instanceof Error ? e.message : String(e));
      setUpdateState('error');
    }
  };

  return (
    <Layout title={t('about.title')} subtitle={t('about.subtitle')}>
      <div className="flex flex-col gap-5 max-w-[700px]">

        {/* App Header */}
        <div className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 p-6">
          <div className="flex items-center gap-4">
            <img
              src="/verbatim-logo.png"
              alt="Verbatim"
              className="w-12 h-12 object-contain flex-shrink-0"
            />
            <div className="flex-1">
              <h2 className="text-slate-900 dark:text-slate-100 font-bold text-lg tracking-tight">Verbatim</h2>
              <a
                href="https://github.com/mkamran67/verbatim-desktop/releases"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400 hover:text-amber-500 dark:hover:text-amber-400 transition-colors mt-0.5"
              >
                <i className="ri-github-fill text-sm" />
                github.com/mkamran67/verbatim-desktop/releases
              </a>
            </div>
            {version && (
              <span className="text-xs font-mono text-slate-400 dark:text-slate-500 bg-slate-50 dark:bg-slate-700 px-2.5 py-1 rounded-lg">
                v{version}
              </span>
            )}
          </div>

          {/* Update Check */}
          <div className="mt-4 pt-4 border-t border-slate-100 dark:border-slate-700 flex items-center gap-3">
            <button
              onClick={checkForUpdate}
              disabled={updateState === 'checking'}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-500/10 hover:bg-amber-100 dark:hover:bg-amber-500/20 border border-amber-200 dark:border-amber-500/30 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <i className={`ri-refresh-line text-sm ${updateState === 'checking' ? 'animate-spin' : ''}`} />
              {updateState === 'checking' ? `${t('about.checkForUpdates')}...` : t('about.checkForUpdates')}
            </button>

            {updateState === 'done' && updateInfo && (
              updateInfo.update_available ? (
                <div className="flex items-center gap-2">
                  <span className="inline-flex items-center gap-1 text-xs font-medium text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-500/10 px-2.5 py-1 rounded-lg">
                    <i className="ri-arrow-up-circle-line text-sm" />
                    v{updateInfo.latest_version} {t('about.available')}
                  </span>
                  <a
                    href={updateInfo.release_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-amber-600 dark:text-amber-400 hover:text-amber-700 dark:hover:text-amber-300 underline"
                  >
                    {t('about.viewRelease')}
                  </a>
                </div>
              ) : (
                <span className="text-xs text-slate-500 dark:text-slate-400">
                  <i className="ri-check-line text-emerald-500 mr-1" />
                  {t('about.upToDate')}
                </span>
              )
            )}

            {updateState === 'error' && (
              <span className="text-xs text-slate-400 dark:text-slate-500">
                {t('about.updateError')}{updateError ? ` — ${updateError}` : ''}
              </span>
            )}
          </div>
        </div>

        {/* Privacy Policy */}
        <Section title={t('about.privacyPolicy')} icon="ri-shield-check-line" defaultOpen>
          <p className="font-semibold text-slate-700 dark:text-slate-300">{t('about.privacyIntro')}</p>
          <ul className="list-disc list-inside space-y-2 mt-2">
            <li>
              <span className="font-medium text-slate-700 dark:text-slate-300">{t('about.privacyOffline')}</span> — {t('about.privacyOfflineDesc')}
            </li>
            <li>
              <span className="font-medium text-slate-700 dark:text-slate-300">{t('about.privacyOptIn')}</span> — {t('about.privacyOptInDesc')}
            </li>
            <li>
              <span className="font-medium text-slate-700 dark:text-slate-300">{t('about.privacyNoTelemetry')}</span> — {t('about.privacyNoTelemetryDesc')}
            </li>
            <li>
              <span className="font-medium text-slate-700 dark:text-slate-300">{t('about.privacyLocalStorage')}</span> — {t('about.privacyLocalStorageDesc')}
            </li>
            <li>
              <span className="font-medium text-slate-700 dark:text-slate-300">{t('about.privacyOpenSource')}</span> — {t('about.privacyOpenSourceDesc')}
            </li>
          </ul>
          <p className="mt-3 text-slate-500 dark:text-slate-500 text-[11px]">
            {t('about.privacyCloudNote')}
          </p>
        </Section>

        {/* Known Bugs */}
        <Section title={t('about.knownBugs')} icon="ri-bug-line">
          <ul className="list-disc list-inside space-y-2">
            <li>{t('about.knownBugPpLanguage')}</li>
          </ul>
        </Section>

        {/* Changelog */}
        <Section title={t('about.changelog')} icon="ri-file-list-3-line">
          <div className="space-y-4">
            <div>
              <div className="flex items-center gap-2 mb-1.5">
                <span className="font-semibold text-slate-700 dark:text-slate-300 text-xs">v0.1.0</span>
                <span className="text-[10px] text-slate-400 dark:text-slate-500">{t('about.initialRelease')}</span>
              </div>
              <ul className="list-disc list-inside space-y-1 text-slate-500 dark:text-slate-400">
                <li>{t('about.changelog1-9')}</li>
              </ul>
            </div>
          </div>
        </Section>

        {/* License */}
        <Section title={t('about.license')} icon="ri-open-source-line">
          <p>
            {t('about.licenseText')}{' '}
            <span className="font-semibold text-slate-700 dark:text-slate-300">{t('about.mitLicense')}</span>.
          </p>
          <p className="mt-2">
            {t('about.licenseDesc')}
          </p>
        </Section>

        {/* Links */}
        <div className="flex items-center gap-3 px-1">
          <a
            href="https://github.com/mkamran67/verbatim-desktop"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400 hover:text-amber-500 dark:hover:text-amber-400 transition-colors"
          >
            <i className="ri-github-fill text-sm" />
            {t('about.github')}
          </a>
          <span className="text-slate-200 dark:text-slate-700">|</span>
          <a
            href="https://github.com/mkamran67/verbatim-desktop/issues"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400 hover:text-amber-500 dark:hover:text-amber-400 transition-colors"
          >
            <i className="ri-bug-line text-sm" />
            {t('about.reportIssue')}
          </a>
        </div>

      </div>
    </Layout>
  );
}
