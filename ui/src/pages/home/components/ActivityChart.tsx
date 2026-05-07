import { useTranslation } from 'react-i18next';
import { useAppSelector } from '@/store/hooks';

export default function ActivityChart() {
  const { t } = useTranslation();
  const recent = useAppSelector((s) => s.transcriptions.recent);

  // Group transcriptions by date
  const byDate: Record<string, number> = {};
  for (const t of recent) {
    const date = t.created_at.slice(0, 10);
    byDate[date] = (byDate[date] || 0) + t.word_count;
  }

  // Always show the last 7 days with today as the rightmost bar
  const days: [string, number][] = [];
  for (let i = 6; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    const key = d.toISOString().slice(0, 10);
    days.push([key, byDate[key] || 0]);
  }

  const maxWords = Math.max(...days.map(([, w]) => w), 1);

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5 h-full flex flex-col">
      <div className="flex items-center justify-between mb-5">
        <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('activity.title')}</h2>
        <span className="text-xs text-slate-400 dark:text-slate-500">{t('activity.subtitle')}</span>
      </div>
      <div className="overflow-x-auto flex-1">
      <div className="flex items-end gap-2 min-w-[350px] h-full">
        {days.map(([date, words]) => {
          const pct = words > 0 ? (words / maxWords) * 100 : 0;
          const shortDate = date.slice(5); // MM-DD
          return (
            <div key={date} className="flex-1 flex flex-col items-center gap-1.5 group">
              <span className="text-[10px] text-slate-500 dark:text-slate-400 font-medium">
                {words > 0 ? words.toLocaleString() : ''}
              </span>
              <div className="relative w-full flex items-end justify-center" style={{ height: '80px' }}>
                <div
                  className={`w-full rounded-t-md transition-all ${words > 0 ? 'bg-amber-400 group-hover:bg-amber-500' : 'bg-slate-100 dark:bg-slate-700'}`}
                  style={{ height: words > 0 ? `${pct}%` : '4px' }}
                />
              </div>
              <span className="text-[10px] text-slate-400 dark:text-slate-500">{shortDate}</span>
            </div>
          );
        })}
      </div>
      </div>
    </div>
  );
}
