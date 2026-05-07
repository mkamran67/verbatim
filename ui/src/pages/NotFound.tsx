import { useLocation, Link } from "react-router-dom";
import { useTranslation } from 'react-i18next';

export default function NotFound() {
  const location = useLocation();
  const { t } = useTranslation();

  return (
    <div className="relative flex flex-col items-center justify-center h-screen text-center px-4 bg-white dark:bg-slate-900">
      <h1 className="absolute bottom-0 text-9xl md:text-[12rem] font-black text-gray-50 dark:text-slate-800 select-none pointer-events-none z-0">
        404
      </h1>
      <div className="relative z-10">
        <h1 className="text-xl md:text-2xl font-semibold mt-6 text-slate-900 dark:text-slate-100">{t('notFound.title')}</h1>
        <p className="mt-2 text-base text-gray-400 dark:text-slate-500 font-mono">{location.pathname}</p>
        <Link to="/" className="mt-4 inline-block text-amber-500 hover:text-amber-600 font-medium">
          {t('notFound.goHome')}
        </Link>
      </div>
    </div>
  );
}
