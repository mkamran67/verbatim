import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { open } from '@tauri-apps/plugin-shell';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { fetchStats, fetchDailyTokenUsage } from '@/store/slices/statsSlice';
import { fetchModelUsage, fetchProviderCosts } from '@/store/slices/transcriptionsSlice';
import { fetchDeepgramBalance, fetchOpenaiCosts } from '@/store/slices/balanceSlice';
import type { DailyTokenUsage } from '@/lib/types';

function formatCost(usd: number): string {
  if (usd === 0) return '$0.00';
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}

function formatTimeAgo(isoString: string): string {
  const diffMs = Date.now() - new Date(isoString).getTime();
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export default function ApiUsage() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const stats = useAppSelector((s) => s.stats.data);
  const rawDailyTokens = useAppSelector((s) => s.stats.dailyTokens);
  const modelUsage = useAppSelector((s) => s.transcriptions.modelUsage);
  const providerCosts = useAppSelector((s) => s.transcriptions.providerCosts);
  const deepgramBalance = useAppSelector((s) => s.balance.deepgram.data);
  const balanceLoading = useAppSelector((s) => s.balance.deepgram.loading);
  const balanceError = useAppSelector((s) => s.balance.deepgram.error);
  const openaiCosts = useAppSelector((s) => s.balance.openai.data);
  const openaiCostsLoading = useAppSelector((s) => s.balance.openai.loading);
  const openaiCostsError = useAppSelector((s) => s.balance.openai.error);
  const [selectedDayIndex, setSelectedDayIndex] = useState(30);

  const providerLabel = (provider: string): string => {
    switch (provider) {
      case 'deepgram': return t('apiUsage.deepgramStt');
      case 'openai-stt': return t('apiUsage.openaiStt');
      case 'openai-postproc': return t('apiUsage.openaiPostProc');
      default: return provider;
    }
  };

  useEffect(() => {
    dispatch(fetchStats());
    dispatch(fetchDailyTokenUsage(31));
    dispatch(fetchModelUsage());
    dispatch(fetchProviderCosts());
  }, [dispatch]);

  // Build full 31-day array filling gaps with zeros
  const days = useMemo(() => {
    const lookup: Record<string, DailyTokenUsage> = {};
    for (const d of rawDailyTokens) lookup[d.date] = d;

    const result: DailyTokenUsage[] = [];
    for (let i = 30; i >= 0; i--) {
      const dt = new Date();
      dt.setDate(dt.getDate() - i);
      const key = dt.toISOString().slice(0, 10);
      result.push(lookup[key] || { date: key, prompt_tokens: 0, completion_tokens: 0 });
    }
    return result;
  }, [rawDailyTokens]);

  const selectedDay = days[selectedDayIndex];

  const formatDate = (dateStr: string) => {
    const d = new Date(dateStr + 'T00:00:00');
    return d.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric', year: 'numeric' });
  };

  const maxDayTokens = Math.max(...days.map((d) => d.prompt_tokens + d.completion_tokens), 1);
  const maxModelTokens = Math.max(...modelUsage.map((m) => m.total_tokens), 1);
  const maxProviderCost = Math.max(...providerCosts.map((p) => p.total_cost_usd), 0.001);

  const tokenCards = [
    {
      label: t('apiUsage.today'),
      value: stats ? stats.today_tokens.toLocaleString() : '—',
      icon: 'ri-time-line',
      color: 'text-orange-500',
      bg: 'bg-orange-50 dark:bg-orange-500/10',
    },
    {
      label: t('apiUsage.thisWeek'),
      value: stats ? stats.week_tokens.toLocaleString() : '—',
      icon: 'ri-calendar-line',
      color: 'text-amber-500',
      bg: 'bg-amber-50 dark:bg-amber-500/10',
    },
    {
      label: t('apiUsage.allTime'),
      value: stats ? stats.total_tokens.toLocaleString() : '—',
      icon: 'ri-coin-line',
      color: 'text-emerald-500',
      bg: 'bg-emerald-50 dark:bg-emerald-500/10',
    },
  ];

  const costCards = [
    {
      label: t('apiUsage.todayCost'),
      value: stats ? formatCost(stats.today_cost_usd) : '—',
      icon: 'ri-money-dollar-circle-line',
      color: 'text-blue-500',
      bg: 'bg-blue-50 dark:bg-blue-500/10',
    },
    {
      label: t('apiUsage.weekCost'),
      value: stats ? formatCost(stats.week_cost_usd) : '—',
      icon: 'ri-funds-line',
      color: 'text-violet-500',
      bg: 'bg-violet-50 dark:bg-violet-500/10',
    },
    {
      label: t('apiUsage.totalSpend'),
      value: stats ? formatCost(stats.total_cost_usd) : '—',
      icon: 'ri-wallet-3-line',
      color: 'text-rose-500',
      bg: 'bg-rose-50 dark:bg-rose-500/10',
    },
  ];

  return (
    <Layout title={t('apiUsage.title')} subtitle={t('apiUsage.subtitle')}>
      <div className="flex flex-col gap-5 max-w-[1200px]">
        {/* Cost summary cards */}
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          {costCards.map((s) => (
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

        {/* Credit Balance */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2">
              <div className="w-7 h-7 flex items-center justify-center rounded-lg bg-emerald-50 dark:bg-emerald-500/10 border border-emerald-100 dark:border-emerald-500/20">
                <i className="ri-bank-card-line text-sm text-emerald-500" />
              </div>
              <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('apiUsage.creditBalance')}</h2>
            </div>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {/* Deepgram Balance */}
            <div className="border border-slate-100 dark:border-slate-700 rounded-lg p-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-slate-700 dark:text-slate-300 text-sm font-medium">{t('apiUsage.deepgram')}</span>
                <button
                  onClick={() => dispatch(fetchDeepgramBalance(true))}
                  disabled={balanceLoading}
                  className="text-xs text-blue-500 hover:text-blue-600 disabled:opacity-50 cursor-pointer"
                >
                  {balanceLoading ? `${t('apiUsage.checkBalance')}...` : t('apiUsage.checkBalance')}
                </button>
              </div>
              {deepgramBalance ? (
                <div>
                  <p className="text-slate-900 dark:text-slate-100 text-xl font-bold tabular-nums">
                    ${deepgramBalance.amount.toFixed(2)} <span className="text-xs font-normal text-slate-400">{deepgramBalance.currency}</span>
                  </p>
                  <p className="text-slate-400 dark:text-slate-500 text-[10px] mt-1">
                    {t('apiUsage.lastChecked', { time: formatTimeAgo(deepgramBalance.checked_at) })}
                    {deepgramBalance.estimated_usage_since > 0 && (
                      <> · {t('apiUsage.estUsageSince', { cost: formatCost(deepgramBalance.estimated_usage_since) })}</>
                    )}
                  </p>
                </div>
              ) : balanceError ? (
                <p className={`text-xs ${balanceError.includes('permissions') ? 'text-amber-500' : 'text-red-500'}`}>{balanceError}</p>
              ) : (
                <p className="text-slate-400 dark:text-slate-500 text-sm">{t('apiUsage.clickToCheck')}</p>
              )}
            </div>
            {/* OpenAI Costs */}
            <div className="border border-slate-100 dark:border-slate-700 rounded-lg p-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-slate-700 dark:text-slate-300 text-sm font-medium">{t('apiUsage.openai')}</span>
                <button
                  onClick={() => dispatch(fetchOpenaiCosts(true))}
                  disabled={openaiCostsLoading}
                  className="text-xs text-blue-500 hover:text-blue-600 disabled:opacity-50 cursor-pointer"
                >
                  {openaiCostsLoading ? `${t('apiUsage.checkCosts')}...` : t('apiUsage.checkCosts')}
                </button>
              </div>
              {openaiCosts ? (
                <div>
                  <p className="text-slate-900 dark:text-slate-100 text-xl font-bold tabular-nums">
                    ${openaiCosts.amount.toFixed(2)} <span className="text-xs font-normal text-slate-400">{t('apiUsage.last30Days')}</span>
                  </p>
                  <p className="text-slate-400 dark:text-slate-500 text-[10px] mt-1">
                    {t('apiUsage.lastChecked', { time: formatTimeAgo(openaiCosts.checked_at) })}
                    {openaiCosts.estimated_usage_since > 0 && (
                      <> · {t('apiUsage.estUsageSince', { cost: formatCost(openaiCosts.estimated_usage_since) })}</>
                    )}
                  </p>
                  <a
                    href="#"
                    onClick={(e) => { e.preventDefault(); open('https://platform.openai.com/settings/organization/billing/overview'); }}
                    className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 text-xs inline-flex items-center gap-0.5 mt-1"
                  >
                    {t('apiUsage.checkBalanceExternal')} <i className="ri-external-link-line text-[10px]" />
                  </a>
                </div>
              ) : openaiCostsError ? (
                <p className={`text-xs ${openaiCostsError.includes('permissions') || openaiCostsError.includes('not configured') ? 'text-amber-500' : 'text-red-500'}`}>{openaiCostsError}</p>
              ) : (
                <p className="text-slate-900 dark:text-slate-100 text-xl font-bold tabular-nums">
                  {stats ? formatCost(
                    providerCosts
                      .filter((p) => p.provider.startsWith('openai'))
                      .reduce((sum, p) => sum + p.total_cost_usd, 0)
                  ) : '—'}
                  <span className="text-xs font-normal text-slate-400 ml-1">estimated</span>
                </p>
              )}
            </div>
          </div>
        </div>

        {/* Cost by Provider */}
        {providerCosts.length > 0 && (
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
            <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700">
              <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('apiUsage.costByProvider')}</h2>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiUsage.costByProviderDesc')}</p>
            </div>
            <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
              {providerCosts.map((p) => {
                const pct = (p.total_cost_usd / maxProviderCost) * 100;
                return (
                  <div key={p.provider} className="px-5 py-3.5 flex flex-col gap-2">
                    <div className="flex items-center justify-between">
                      <span className="text-slate-800 dark:text-slate-200 text-sm font-medium">{providerLabel(p.provider)}</span>
                      <div className="flex items-center gap-4">
                        {p.total_duration_secs > 0 && (
                          <span className="text-slate-400 text-xs tabular-nums">{(p.total_duration_secs / 60).toFixed(1)} min</span>
                        )}
                        <span className="text-slate-500 text-xs tabular-nums">{p.total_requests} requests</span>
                        <span className="text-slate-600 dark:text-slate-300 text-xs font-semibold tabular-nums w-20 text-right">{formatCost(p.total_cost_usd)}</span>
                      </div>
                    </div>
                    <div className="bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                      <div className="h-full bg-blue-400 rounded-full" style={{ width: `${pct}%` }} />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Token summary cards */}
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          {tokenCards.map((s) => (
            <div key={s.label} className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
              <div className="flex items-start justify-between mb-3">
                <div className={`w-9 h-9 flex items-center justify-center rounded-lg ${s.bg}`}>
                  <i className={`${s.icon} ${s.color} text-base`} />
                </div>
              </div>
              <p className="text-slate-900 dark:text-slate-100 text-2xl font-bold tabular-nums">{s.value}</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-1">{s.label} ({t('apiUsage.tokens')})</p>
            </div>
          ))}
        </div>

        {/* Daily token chart */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2">
              <div className="w-7 h-7 flex items-center justify-center rounded-lg bg-orange-50 dark:bg-orange-500/10 border border-orange-100 dark:border-orange-500/20">
                <i className="ri-bar-chart-2-line text-sm text-orange-500" />
              </div>
              <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('apiUsage.dailyTokenUsage')}</h2>
            </div>
            <div className="flex items-center gap-5">
              <div className="flex items-center gap-1.5">
                <span className="w-2.5 h-2.5 rounded-sm bg-orange-400" />
                <span className="text-slate-500 dark:text-slate-400 text-[10px]">{t('apiUsage.inputTokens')}</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="w-2.5 h-2.5 rounded-sm bg-amber-300" />
                <span className="text-slate-500 dark:text-slate-400 text-[10px]">{t('apiUsage.outputTokens')}</span>
              </div>
            </div>
          </div>

          <div className="overflow-x-auto">
          <div className="flex items-end gap-1.5 min-w-[600px]" style={{ height: '140px' }}>
            {days.map((day, idx) => {
              const total = day.prompt_tokens + day.completion_tokens;
              const inputPct = (day.prompt_tokens / maxDayTokens) * 100;
              const outputPct = (day.completion_tokens / maxDayTokens) * 100;
              const shortDate = day.date.slice(5);
              const isSelected = idx === selectedDayIndex;
              return (
                <div
                  key={day.date}
                  className={`flex-1 flex flex-col items-center gap-1 group cursor-pointer rounded-lg transition-all ${isSelected ? 'bg-slate-50 dark:bg-slate-700/50' : ''}`}
                  onClick={() => setSelectedDayIndex(idx)}
                >
                  <div className="relative w-full flex flex-col items-center justify-end" style={{ height: '110px' }}>
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
                        {t('stats.input')}: {day.prompt_tokens.toLocaleString()} · {t('stats.output')}: {day.completion_tokens.toLocaleString()}
                      </div>
                    )}
                  </div>
                  <span className={`text-[9px] ${isSelected ? 'text-orange-500 font-semibold' : 'text-slate-400 dark:text-slate-500'}`}>{shortDate}</span>
                </div>
              );
            })}
          </div>
          </div>

          {/* Selected day detail */}
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-3">
              <button
                onClick={() => setSelectedDayIndex((i) => Math.max(0, i - 1))}
                disabled={selectedDayIndex === 0}
                className="w-8 h-8 flex items-center justify-center rounded-lg bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-600 dark:text-slate-300 disabled:opacity-30 disabled:cursor-not-allowed transition-all"
              >
                <i className="ri-arrow-left-s-line text-base" />
              </button>
              <span className="text-slate-700 dark:text-slate-300 text-sm font-medium min-w-[220px] text-center">
                {selectedDay ? formatDate(selectedDay.date) : '—'}
              </span>
              <button
                onClick={() => setSelectedDayIndex((i) => Math.min(30, i + 1))}
                disabled={selectedDayIndex === 30}
                className="w-8 h-8 flex items-center justify-center rounded-lg bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-600 dark:text-slate-300 disabled:opacity-30 disabled:cursor-not-allowed transition-all"
              >
                <i className="ri-arrow-right-s-line text-base" />
              </button>
            </div>
            {selectedDay && (
              <div className="flex items-center gap-6">
                <div className="text-right">
                  <p className="text-slate-900 dark:text-slate-100 text-sm font-bold tabular-nums">{selectedDay.prompt_tokens.toLocaleString()}</p>
                  <p className="text-slate-400 dark:text-slate-500 text-[10px]">{t('apiUsage.inputTokens')}</p>
                </div>
                <div className="text-right">
                  <p className="text-slate-900 dark:text-slate-100 text-sm font-bold tabular-nums">{selectedDay.completion_tokens.toLocaleString()}</p>
                  <p className="text-slate-400 dark:text-slate-500 text-[10px]">{t('apiUsage.outputTokens')}</p>
                </div>
                <div className="text-right">
                  <p className="text-slate-900 dark:text-slate-100 text-sm font-bold tabular-nums">{(selectedDay.prompt_tokens + selectedDay.completion_tokens).toLocaleString()}</p>
                  <p className="text-slate-400 dark:text-slate-500 text-[10px]">{t('stats.total')}</p>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Usage by model */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
          <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700">
            <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('apiUsage.usageByModel')}</h2>
            <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('apiUsage.usageByModelDesc')}</p>
          </div>
          {modelUsage.length === 0 ? (
            <div className="px-5 py-8 text-center">
              <p className="text-slate-400 dark:text-slate-500 text-sm">{t('apiUsage.noUsage')}</p>
            </div>
          ) : (
            <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
              {modelUsage.map((m) => {
                const pct = (m.total_tokens / maxModelTokens) * 100;
                return (
                  <div key={m.model} className="px-5 py-3.5 flex flex-col gap-2">
                    <div className="flex items-center justify-between">
                      <span className="text-slate-800 dark:text-slate-200 text-sm font-medium font-mono">{m.model}</span>
                      <div className="flex items-center gap-4">
                        <span className="text-orange-500 text-xs tabular-nums">{m.prompt_tokens.toLocaleString()} {t('stats.input')}</span>
                        <span className="text-amber-500 text-xs tabular-nums">{m.completion_tokens.toLocaleString()} {t('stats.output')}</span>
                        <span className="text-slate-600 dark:text-slate-300 text-xs font-semibold tabular-nums w-20 text-right">{m.total_tokens.toLocaleString()}</span>
                      </div>
                    </div>
                    <div className="bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                      <div className="h-full bg-orange-400 rounded-full" style={{ width: `${pct}%` }} />
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
