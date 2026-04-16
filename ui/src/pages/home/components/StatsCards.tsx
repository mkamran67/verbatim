import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { fetchDailyTokenUsage } from '@/store/slices/statsSlice';
import type { DailyTokenUsage } from '@/lib/types';

export default function StatsCards() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const stats = useAppSelector((s) => s.stats.data);
  const rawDailyTokens = useAppSelector((s) => s.stats.dailyTokens);

  useEffect(() => {
    dispatch(fetchDailyTokenUsage(7));
  }, [dispatch]);

  // Always show 7 days with today as the rightmost
  const dailyTokens = useMemo(() => {
    const lookup: Record<string, DailyTokenUsage> = {};
    for (const d of rawDailyTokens) lookup[d.date] = d;

    const days: DailyTokenUsage[] = [];
    for (let i = 6; i >= 0; i--) {
      const dt = new Date();
      dt.setDate(dt.getDate() - i);
      const key = dt.toISOString().slice(0, 10);
      days.push(lookup[key] || { date: key, prompt_tokens: 0, completion_tokens: 0 });
    }
    return days;
  }, [rawDailyTokens]);

  const cards = [
    {
      label: t('stats.totalTranscriptions'),
      value: stats ? stats.total_transcriptions.toLocaleString() : '—',
      change: stats ? t('stats.todayCount', { count: stats.today_transcriptions }) : '',
      icon: 'ri-file-text-line',
      color: 'text-amber-500',
      bg: 'bg-amber-50 dark:bg-amber-500/10',
      border: 'border-amber-100 dark:border-amber-500/20',
    },
    {
      label: t('stats.totalWords'),
      value: stats ? stats.total_words.toLocaleString() : '—',
      change: stats ? t('stats.todayWords', { count: stats.today_words }) : '',
      icon: 'ri-font-size',
      color: 'text-emerald-500',
      bg: 'bg-emerald-50 dark:bg-emerald-500/10',
      border: 'border-emerald-100 dark:border-emerald-500/20',
    },
    {
      label: t('stats.thisWeek'),
      value: stats ? stats.week_transcriptions.toLocaleString() : '—',
      change: stats ? t('stats.weekWords', { count: stats.week_words }) : '',
      icon: 'ri-calendar-line',
      color: 'text-sky-500',
      bg: 'bg-sky-50 dark:bg-sky-500/10',
      border: 'border-sky-100 dark:border-sky-500/20',
    },
    {
      label: t('stats.today'),
      value: stats ? stats.today_words.toLocaleString() : '—',
      change: stats ? t('stats.todayTranscriptions', { count: stats.today_transcriptions }) : '',
      icon: 'ri-time-line',
      color: 'text-violet-500',
      bg: 'bg-violet-50 dark:bg-violet-500/10',
      border: 'border-violet-100 dark:border-violet-500/20',
    },
  ];

  const maxTokens = Math.max(
    ...dailyTokens.map((d) => d.prompt_tokens + d.completion_tokens),
    1
  );

  const totalInput = dailyTokens.reduce((s, d) => s + d.prompt_tokens, 0);
  const totalOutput = dailyTokens.reduce((s, d) => s + d.completion_tokens, 0);

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {cards.map((s) => (
          <div
            key={s.label}
            className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5 flex flex-col gap-4"
          >
            <div className="flex items-start justify-between">
              <div>
                <p className="text-slate-500 dark:text-slate-400 text-xs font-medium">{s.label}</p>
                <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold mt-1 tabular-nums">{s.value}</p>
              </div>
              <div className={`w-9 h-9 flex items-center justify-center rounded-lg ${s.bg} border ${s.border}`}>
                <i className={`${s.icon} text-base ${s.color}`} />
              </div>
            </div>
            <p className="text-xs text-slate-400 dark:text-slate-500">{s.change}</p>
          </div>
        ))}
      </div>

      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <div className="w-7 h-7 flex items-center justify-center rounded-lg bg-orange-50 dark:bg-orange-500/10 border border-orange-100 dark:border-orange-500/20">
              <i className="ri-coin-line text-sm text-orange-500" />
            </div>
            <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('stats.tokenUsage')}</h2>
          </div>
          <div className="flex items-center gap-5">
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded-sm bg-orange-400" />
              <span className="text-slate-500 dark:text-slate-400 text-[10px]">{t('stats.input')} ({totalInput.toLocaleString()})</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="w-2.5 h-2.5 rounded-sm bg-amber-300" />
              <span className="text-slate-500 dark:text-slate-400 text-[10px]">{t('stats.output')} ({totalOutput.toLocaleString()})</span>
            </div>
            <span className="text-slate-400 dark:text-slate-500 text-[10px]">{t('stats.total')}: {(totalInput + totalOutput).toLocaleString()}</span>
          </div>
        </div>

        <div className="overflow-x-auto">
        <div className="flex items-end gap-1.5 min-w-[400px]" style={{ height: '100px' }}>
          {dailyTokens.map((day) => {
            const total = day.prompt_tokens + day.completion_tokens;
            const inputPct = (day.prompt_tokens / maxTokens) * 100;
            const outputPct = (day.completion_tokens / maxTokens) * 100;
            const shortDate = day.date.slice(5);
            return (
              <div key={day.date} className="flex-1 flex flex-col items-center gap-1 group">
                <div className="relative w-full flex flex-col items-center justify-end" style={{ height: '80px' }}>
                  {total > 0 ? (
                    <div className="w-full flex flex-col-reverse items-center h-full">
                      <div
                        className="w-full rounded-b-sm bg-orange-400 group-hover:bg-orange-500 transition-all"
                        style={{ height: `${inputPct}%`, minHeight: '2px' }}
                      />
                      <div
                        className="w-full rounded-t-sm bg-amber-300 group-hover:bg-amber-400 transition-all"
                        style={{ height: `${outputPct}%`, minHeight: '2px' }}
                      />
                    </div>
                  ) : (
                    <div className="w-full bg-slate-100 dark:bg-slate-700 rounded-sm" style={{ height: '4px' }} />
                  )}
                  {total > 0 && (
                    <div className="absolute -top-8 left-1/2 -translate-x-1/2 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                      In: {day.prompt_tokens.toLocaleString()} · Out: {day.completion_tokens.toLocaleString()}
                    </div>
                  )}
                </div>
                <span className="text-[9px] text-slate-400 dark:text-slate-500">{shortDate}</span>
              </div>
            );
          })}
        </div>
        </div>
      </div>
    </div>
  );
}
