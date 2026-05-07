import { Link, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

const mainNav = [
  { path: '/', icon: 'ri-home-5-line', labelKey: 'sidebar.home' },
  { path: '/recordings', icon: 'ri-mic-line', labelKey: 'sidebar.recordings' },
  { path: '/word-count', icon: 'ri-bar-chart-2-line', labelKey: 'sidebar.wordCount' },
  { path: '/api-usage', icon: 'ri-bar-chart-box-line', labelKey: 'sidebar.apiUsage' },
];

const bottomNav = [
  { path: '/speech-to-text', icon: 'ri-voiceprint-line', labelKey: 'sidebar.speechToText' },
  { path: '/post-processing', icon: 'ri-magic-line', labelKey: 'sidebar.postProcessing' },
  { path: '/settings', icon: 'ri-settings-3-line', labelKey: 'sidebar.settings' },
  { path: '/api-keys', icon: 'ri-key-2-line', labelKey: 'sidebar.apiKeys' },
  { path: '/about', icon: 'ri-information-line', labelKey: 'sidebar.about' },
];

export default function Sidebar() {
  const location = useLocation();
  const { t } = useTranslation();

  const isActive = (path: string) =>
    path === '/' ? location.pathname === '/' : location.pathname.startsWith(path);

  return (
    <aside className="w-[240px] min-w-[240px] h-screen flex flex-col bg-[#0f172a] dark:bg-slate-950 overflow-hidden">
      {/* Logo */}
      <div className="px-5 py-6 flex items-center gap-3 border-b border-white/10">
        <img
          src="/verbatim-logo.png"
          alt="Verbatim"
          className="w-8 h-8 object-contain"
        />
        <span className="text-white font-bold text-lg tracking-tight" style={{ fontFamily: "'Inter', sans-serif" }}>
          Verbatim
        </span>
      </div>

      {/* Main Navigation */}
      <nav className="flex-1 px-3 py-4 flex flex-col gap-0.5 overflow-y-auto">
        <p className="text-[10px] font-semibold text-white/30 uppercase tracking-widest px-3 mb-2">{t('sidebar.menu')}</p>
        {mainNav.map((item) => (
          <Link
            key={item.path}
            to={item.path}
            className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all cursor-pointer whitespace-nowrap ${
              isActive(item.path)
                ? 'bg-amber-500/15 text-amber-400'
                : 'text-white/60 hover:text-white hover:bg-white/5'
            }`}
          >
            <div className="w-5 h-5 flex items-center justify-center">
              <i className={`${item.icon} text-base`} />
            </div>
            {t(item.labelKey)}
            {isActive(item.path) && (
              <span className="ml-auto w-1.5 h-1.5 rounded-full bg-amber-400" />
            )}
          </Link>
        ))}
      </nav>

      {/* Bottom Navigation */}
      <div className="px-3 pb-4 border-t border-white/10 pt-3 flex flex-col gap-0.5">
        {bottomNav.map((item) => (
          <Link
            key={item.path}
            to={item.path}
            className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all cursor-pointer whitespace-nowrap ${
              isActive(item.path)
                ? 'bg-amber-500/15 text-amber-400'
                : 'text-white/60 hover:text-white hover:bg-white/5'
            }`}
          >
            <div className="w-5 h-5 flex items-center justify-center">
              <i className={`${item.icon} text-base`} />
            </div>
            {t(item.labelKey)}
          </Link>
        ))}
      </div>
    </aside>
  );
}
