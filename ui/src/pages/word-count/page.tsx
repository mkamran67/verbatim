import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { api } from '@/lib/tauri';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { fetchStats, fetchDailyWordStats } from '@/store/slices/statsSlice';
import type { DailyWordStats, Transcription } from '@/lib/types';

export default function WordCount() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const stats = useAppSelector((s) => s.stats.data);
  const dailyStats = useAppSelector((s) => s.stats.dailyWords);
  const [transcriptions, setTranscriptions] = useState<Transcription[]>([]);
  const [selectedDayIndex, setSelectedDayIndex] = useState(30); // default to today (last in 31-day window)

  useEffect(() => {
    dispatch(fetchStats());
    dispatch(fetchDailyWordStats(31));
  }, [dispatch]);

  // Build a full 31-day array, filling gaps with zeros
  const days = useMemo(() => {
    const lookup: Record<string, DailyWordStats> = {};
    for (const d of dailyStats) lookup[d.date] = d;

    const result: DailyWordStats[] = [];
    for (let i = 30; i >= 0; i--) {
      const dt = new Date();
      dt.setDate(dt.getDate() - i);
      const key = dt.toISOString().slice(0, 10);
      result.push(lookup[key] || { date: key, total_words: 0, total_transcriptions: 0, total_duration_secs: 0 });
    }
    return result;
  }, [dailyStats]);

  // Fetch transcriptions for the selected day
  useEffect(() => {
    const date = days[selectedDayIndex]?.date;
    if (date) {
      api.getTranscriptionsForDate(date).then(setTranscriptions).catch(() => {});
    }
  }, [selectedDayIndex, days]);

  const selectedDay = days[selectedDayIndex];

  const formatDate = (dateStr: string) => {
    const d = new Date(dateStr + 'T00:00:00');
    return d.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' });
  };

  const formatDuration = (secs: number) => {
    if (secs < 60) return `${Math.round(secs)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.round(secs % 60);
    return s > 0 ? `${m}m ${s}s` : `${m}m`;
  };

  const totalWords = transcriptions.reduce((s, t) => s + t.word_count, 0);
  const avgWords = transcriptions.length > 0 ? Math.round(totalWords / transcriptions.length) : 0;
  const maxWords = Math.max(...transcriptions.map((t) => t.word_count), 0);

  const summaryStats = [
    { label: t('wordCount.totalWords'), value: stats ? stats.total_words.toLocaleString() : '—', icon: 'ri-text', color: 'text-amber-500', bg: 'bg-amber-50 dark:bg-amber-500/10' },
    { label: t('wordCount.avgPerTranscription'), value: avgWords.toLocaleString(), icon: 'ri-bar-chart-grouped-line', color: 'text-emerald-500', bg: 'bg-emerald-50 dark:bg-emerald-500/10' },
    { label: t('wordCount.thisWeek'), value: stats ? stats.week_words.toLocaleString() : '—', icon: 'ri-calendar-line', color: 'text-sky-500', bg: 'bg-sky-50 dark:bg-sky-500/10' },
    { label: t('wordCount.totalTranscriptions'), value: stats ? stats.total_transcriptions.toLocaleString() : '—', icon: 'ri-trophy-line', color: 'text-violet-500', bg: 'bg-violet-50 dark:bg-violet-500/10' },
  ];

  const sorted = [...transcriptions].sort((a, b) => b.word_count - a.word_count);

  return (
    <Layout title={t('wordCount.title')} subtitle={t('wordCount.subtitle')}>
      <div className="flex flex-col gap-5 max-w-[1200px]">
        {/* Summary stats */}
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
          {summaryStats.map((s) => (
            <div key={s.label} className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
              <div className="flex items-start justify-between mb-3">
                <div className={`w-9 h-9 flex items-center justify-center rounded-lg ${s.bg}`}>
                  <i className={`${s.icon} ${s.color} text-base`} />
                </div>
              </div>
              <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold tabular-nums">{s.value}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-1">{s.label}</p>
            </div>
          ))}
        </div>

        {/* Daily stats navigator */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('wordCount.dailyStats')}</h2>
            <div className="flex items-center gap-3">
              <button
                onClick={() => setSelectedDayIndex(i => Math.max(0, i - 1))}
                disabled={selectedDayIndex === 0}
                className="w-8 h-8 flex items-center justify-center rounded-lg bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-600 dark:text-slate-300 disabled:opacity-30 disabled:cursor-not-allowed transition-all"
              >
                <i className="ri-arrow-left-s-line text-base" />
              </button>
              <span className="text-slate-700 dark:text-slate-300 text-sm font-medium min-w-[220px] text-center">
                {selectedDay ? formatDate(selectedDay.date) : '—'}
              </span>
              <button
                onClick={() => setSelectedDayIndex(i => Math.min(30, i + 1))}
                disabled={selectedDayIndex === 30}
                className="w-8 h-8 flex items-center justify-center rounded-lg bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-600 dark:text-slate-300 disabled:opacity-30 disabled:cursor-not-allowed transition-all"
              >
                <i className="ri-arrow-right-s-line text-base" />
              </button>
            </div>
          </div>

          {selectedDay && (
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
              <div className="bg-slate-50 dark:bg-slate-700/50 rounded-lg p-4">
                <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold tabular-nums">{selectedDay.total_words.toLocaleString()}</p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-1">{t('wordCount.words')}</p>
              </div>
              <div className="bg-slate-50 dark:bg-slate-700/50 rounded-lg p-4">
                <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold tabular-nums">{selectedDay.total_transcriptions.toLocaleString()}</p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-1">{t('wordCount.transcriptions')}</p>
              </div>
              <div className="bg-slate-50 dark:bg-slate-700/50 rounded-lg p-4">
                <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold tabular-nums">{formatDuration(selectedDay.total_duration_secs)}</p>
                <p className="text-slate-400 dark:text-slate-500 text-xs mt-1">{t('wordCount.duration')}</p>
              </div>
            </div>
          )}
        </div>

        {/* Per-transcription breakdown */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
          <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700">
            <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('wordCount.byTranscription')}</h2>
            <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('wordCount.sortedByWordCount')} — {selectedDay ? formatDate(selectedDay.date) : ''} ({transcriptions.length})</p>
          </div>
          {sorted.length === 0 ? (
            <div className="px-5 py-8 text-center">
              <p className="text-slate-400 dark:text-slate-500 text-sm">{t('wordCount.noTranscriptions')}</p>
            </div>
          ) : (
            <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
              {sorted.map((item) => {
                const pct = maxWords > 0 ? (item.word_count / maxWords) * 100 : 0;
                return (
                  <div key={item.id} className="px-5 py-3 flex flex-col gap-1.5">
                    <div className="flex items-center justify-between">
                      <p className="text-slate-700 dark:text-slate-300 text-xs font-medium truncate max-w-[400px]">{item.text.slice(0, 60)}...</p>
                      <span className="text-slate-600 dark:text-slate-300 text-xs font-semibold tabular-nums">{item.word_count.toLocaleString()}</span>
                    </div>
                    <div className="bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                      <div
                        className="h-full bg-amber-400 rounded-full"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </Layout>
  );
}
