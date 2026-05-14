import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import { open } from '@tauri-apps/plugin-shell';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { fetchStats, fetchDailyTokenUsage, fetchDailyProviderUsage } from '@/store/slices/statsSlice';
import { fetchModelUsage, fetchProviderCosts } from '@/store/slices/transcriptionsSlice';
import { fetchDeepgramBalance } from '@/store/slices/balanceSlice';
import type { DailyTokenUsage, DailyProviderUsage } from '@/lib/types';
import {
  STT_TOKENS_PER_SEC,
  ESTIMATED_TOKEN_PROVIDERS,
  estimatedTokensForProvider,
  providerRole,
} from '@/lib/tokenEstimate';
import DayUsageTooltip from '@/components/feature/DayUsageTooltip';
import HoverTooltip from '@/components/feature/HoverTooltip';

const PROVIDER_META: Record<string, { label: string; color: string; bar: string; barHover: string; dot: string }> = {
  'openai-stt':       { label: 'OpenAI Whisper',  color: '#10a37f', bar: 'bg-[#10a37f]', barHover: 'group-hover:bg-[#0d8a6a]', dot: 'bg-[#10a37f]' },
  'openai-postproc':  { label: 'OpenAI GPT',      color: '#1a7f64', bar: 'bg-[#1a7f64]', barHover: 'group-hover:bg-[#125746]', dot: 'bg-[#1a7f64]' },
  'deepgram':         { label: 'Deepgram',        color: '#7c3aed', bar: 'bg-[#7c3aed]', barHover: 'group-hover:bg-[#6d28d9]', dot: 'bg-[#7c3aed]' },
  'smallest':         { label: 'Smallest',        color: '#ec4899', bar: 'bg-[#ec4899]', barHover: 'group-hover:bg-[#db2777]', dot: 'bg-[#ec4899]' },
  'ollama':           { label: 'Ollama',          color: '#64748b', bar: 'bg-[#64748b]', barHover: 'group-hover:bg-[#475569]', dot: 'bg-[#64748b]' },
};

function providerMeta(id: string) {
  return PROVIDER_META[id] ?? { label: id, color: '#94a3b8', bar: 'bg-slate-400', barHover: 'group-hover:bg-slate-500', dot: 'bg-slate-400' };
}

function estimateTokens(p: DailyProviderUsage): number {
  const real = p.prompt_tokens + p.completion_tokens;
  if (real > 0) return real;
  return estimatedTokensForProvider(p.provider, p.duration_secs);
}

function formatCost(usd: number): string {
  if (usd === 0) return '$0.00';
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}

interface ManualBalance {
  initial: number;
  baselineSpend: number;
  setAt: string;
}

function loadManualBalance(provider: string): ManualBalance | null {
  try {
    const raw = localStorage.getItem(`manualBalance.${provider}`);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (typeof parsed.initial !== 'number' || typeof parsed.baselineSpend !== 'number') return null;
    return parsed as ManualBalance;
  } catch {
    return null;
  }
}

function saveManualBalance(provider: string, mb: ManualBalance | null) {
  if (mb === null) {
    localStorage.removeItem(`manualBalance.${provider}`);
  } else {
    localStorage.setItem(`manualBalance.${provider}`, JSON.stringify(mb));
  }
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
  const dailyProviderUsage = useAppSelector((s) => s.stats.dailyProviderUsage);
  const modelUsage = useAppSelector((s) => s.transcriptions.modelUsage);
  const providerCosts = useAppSelector((s) => s.transcriptions.providerCosts);
  const deepgramBalance = useAppSelector((s) => s.balance.deepgram.data);
  const balanceLoading = useAppSelector((s) => s.balance.deepgram.loading);
  const balanceError = useAppSelector((s) => s.balance.deepgram.error);
  const [selectedDayIndex, setSelectedDayIndex] = useState(30);

  type ManualProvider = 'deepgram' | 'openai' | 'smallest';

  const [manualDeepgram, setManualDeepgram] = useState<ManualBalance | null>(() => loadManualBalance('deepgram'));
  const [manualOpenai, setManualOpenai] = useState<ManualBalance | null>(() => loadManualBalance('openai'));
  const [manualSmallest, setManualSmallest] = useState<ManualBalance | null>(() => loadManualBalance('smallest'));
  const [editingProvider, setEditingProvider] = useState<ManualProvider | null>(null);
  const [editValue, setEditValue] = useState('');

  const providerSpend = useCallback((provider: ManualProvider): number => {
    if (provider === 'deepgram') {
      return providerCosts
        .filter((p) => p.provider === 'deepgram')
        .reduce((sum, p) => sum + p.total_cost_usd, 0);
    }
    if (provider === 'smallest') {
      return providerCosts
        .filter((p) => p.provider === 'smallest')
        .reduce((sum, p) => sum + p.total_cost_usd, 0);
    }
    return providerCosts
      .filter((p) => p.provider.startsWith('openai'))
      .reduce((sum, p) => sum + p.total_cost_usd, 0);
  }, [providerCosts]);

  const manualBalanceFor = (provider: ManualProvider): ManualBalance | null => {
    if (provider === 'deepgram') return manualDeepgram;
    if (provider === 'openai') return manualOpenai;
    return manualSmallest;
  };

  const setManualFor = (provider: ManualProvider, mb: ManualBalance | null) => {
    if (provider === 'deepgram') setManualDeepgram(mb);
    else if (provider === 'openai') setManualOpenai(mb);
    else setManualSmallest(mb);
  };

  const openEditor = (provider: ManualProvider) => {
    const existing = manualBalanceFor(provider);
    setEditValue(existing ? existing.initial.toString() : '');
    setEditingProvider(provider);
  };

  const saveEditor = () => {
    if (!editingProvider) return;
    const trimmed = editValue.trim();
    if (trimmed === '') {
      saveManualBalance(editingProvider, null);
      setManualFor(editingProvider, null);
      setEditingProvider(null);
      return;
    }
    const parsed = parseFloat(trimmed);
    if (!Number.isFinite(parsed) || parsed < 0) return;
    const mb: ManualBalance = {
      initial: parsed,
      baselineSpend: providerSpend(editingProvider),
      setAt: new Date().toISOString(),
    };
    saveManualBalance(editingProvider, mb);
    setManualFor(editingProvider, mb);
    setEditingProvider(null);
  };

  const providerLabel = (provider: string): string => {
    switch (provider) {
      case 'deepgram': return t('apiUsage.deepgramStt');
      case 'openai-stt': return t('apiUsage.openaiStt');
      case 'openai-postproc': return t('apiUsage.openaiPostProc');
      case 'smallest': return t('apiUsage.smallestStt');
      default: return provider;
    }
  };

  useEffect(() => {
    dispatch(fetchStats());
    dispatch(fetchDailyTokenUsage(31));
    dispatch(fetchDailyProviderUsage(31));
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

  // Build 31-day per-provider matrix: { date, byProvider: { provider: {tokens, duration} } }
  const providerDays = useMemo(() => {
    const byDate: Record<string, Record<string, { tokens: number; duration: number; estimated: boolean }>> = {};
    for (const row of dailyProviderUsage) {
      const tokens = estimateTokens(row);
      const estimated = ESTIMATED_TOKEN_PROVIDERS.has(row.provider);
      if (!byDate[row.date]) byDate[row.date] = {};
      byDate[row.date][row.provider] = { tokens, duration: row.duration_secs, estimated };
    }
    const result: { date: string; entries: { provider: string; tokens: number; duration: number; estimated: boolean }[]; totalTokens: number }[] = [];
    for (let i = 30; i >= 0; i--) {
      const dt = new Date();
      dt.setDate(dt.getDate() - i);
      const key = dt.toISOString().slice(0, 10);
      const entries = Object.entries(byDate[key] ?? {})
        .map(([provider, v]) => ({ provider, ...v }))
        .sort((a, b) => b.tokens - a.tokens);
      const totalTokens = entries.reduce((s, e) => s + e.tokens, 0);
      result.push({ date: key, entries, totalTokens });
    }
    return result;
  }, [dailyProviderUsage]);

  const maxProviderDayTokens = Math.max(...providerDays.map((d) => d.totalTokens), 1);

  // Per-date provider rows for the daily-token-chart hover tooltip.
  const providerRowsByDate = useMemo(() => {
    const map: Record<string, DailyProviderUsage[]> = {};
    for (const row of dailyProviderUsage) {
      (map[row.date] ||= []).push(row);
    }
    return map;
  }, [dailyProviderUsage]);

  // Audio seconds aggregated per provider over the window
  const audioByProvider = useMemo(() => {
    const sums: Record<string, number> = {};
    for (const row of dailyProviderUsage) {
      sums[row.provider] = (sums[row.provider] ?? 0) + row.duration_secs;
    }
    return Object.entries(sums)
      .map(([provider, seconds]) => ({ provider, seconds }))
      .filter((e) => e.seconds > 0)
      .sort((a, b) => b.seconds - a.seconds);
  }, [dailyProviderUsage]);

  const maxAudioSeconds = Math.max(...audioByProvider.map((p) => p.seconds), 1);

  const activeProviders = useMemo(() => {
    const set = new Set<string>();
    for (const row of dailyProviderUsage) {
      if (estimateTokens(row) > 0) set.add(row.provider);
    }
    return Array.from(set);
  }, [dailyProviderUsage]);

  const hasEstimatedProviders = activeProviders.some((p) => ESTIMATED_TOKEN_PROVIDERS.has(p));

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

  const renderManualBalance = (provider: ManualProvider) => {
    const mb = manualBalanceFor(provider);
    const isEditing = editingProvider === provider;

    if (isEditing) {
      return (
        <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-700/70 flex items-center gap-2">
          <span className="text-slate-500 dark:text-slate-400 text-xs">$</span>
          <input
            type="number"
            step="0.01"
            min="0"
            value={editValue}
            onChange={(e) => setEditValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') saveEditor();
              if (e.key === 'Escape') setEditingProvider(null);
            }}
            placeholder={t('apiUsage.manualBalancePlaceholder')}
            autoFocus
            className="flex-1 min-w-0 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded px-2 py-1 text-xs text-slate-900 dark:text-slate-100 tabular-nums focus:outline-none focus:border-blue-400"
          />
          <button
            onClick={saveEditor}
            className="text-xs text-blue-500 hover:text-blue-600 cursor-pointer"
          >
            {t('common.save')}
          </button>
          <button
            onClick={() => setEditingProvider(null)}
            className="text-xs text-slate-400 hover:text-slate-500 cursor-pointer"
          >
            {t('common.cancel')}
          </button>
        </div>
      );
    }

    if (!mb) return null;

    const spent = Math.max(0, providerSpend(provider) - mb.baselineSpend);
    const remaining = mb.initial - spent;
    const remainingClass = remaining <= 0
      ? 'text-red-500'
      : remaining < mb.initial * 0.2
        ? 'text-amber-500'
        : 'text-emerald-500';

    return (
      <div className="mt-3 pt-3 border-t border-slate-100 dark:border-slate-700/70">
        <div className="flex items-center justify-between">
          <span className="text-slate-500 dark:text-slate-400 text-[10px] uppercase tracking-wide">
            {t('apiUsage.manualBalance')}
          </span>
          <span className={`text-sm font-bold tabular-nums ${remainingClass}`}>
            {formatCost(remaining)}
          </span>
        </div>
        <p className="text-slate-400 dark:text-slate-500 text-[10px] mt-0.5">
          {t('apiUsage.manualBalanceDetail', {
            initial: formatCost(mb.initial),
            spent: formatCost(spent),
          })}
        </p>
      </div>
    );
  };

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
                <div className="flex items-center gap-3">
                  <button
                    onClick={() => openEditor('deepgram')}
                    title={t('apiUsage.editManualBalance')}
                    className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                  >
                    <i className="ri-pencil-line text-sm" />
                  </button>
                  <button
                    onClick={() => dispatch(fetchDeepgramBalance(true))}
                    disabled={balanceLoading}
                    className="text-xs text-blue-500 hover:text-blue-600 disabled:opacity-50 cursor-pointer"
                  >
                    {balanceLoading ? `${t('apiUsage.checkBalance')}...` : t('apiUsage.checkBalance')}
                  </button>
                </div>
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
              {renderManualBalance('deepgram')}
            </div>
            {/* OpenAI Balance */}
            <div className="border border-slate-100 dark:border-slate-700 rounded-lg p-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-slate-700 dark:text-slate-300 text-sm font-medium">{t('apiUsage.openai')}</span>
                <div className="flex items-center gap-3">
                  <button
                    onClick={() => openEditor('openai')}
                    title={t('apiUsage.editManualBalance')}
                    className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                  >
                    <i className="ri-pencil-line text-sm" />
                  </button>
                </div>
              </div>
              {(() => {
                const estimatedSpend = providerCosts
                  .filter((p) => p.provider.startsWith('openai'))
                  .reduce((sum, p) => sum + p.total_cost_usd, 0);
                return (
                  <div>
                    <p className="text-slate-900 dark:text-slate-100 text-xl font-bold tabular-nums">
                      {formatCost(estimatedSpend)}
                    </p>
                    <p className="text-slate-400 dark:text-slate-500 text-[10px] mt-1">
                      {t('apiUsage.estimatedFromUsage')}
                    </p>
                    <a
                      href="#"
                      onClick={(e) => { e.preventDefault(); open('https://platform.openai.com/settings/organization/billing/overview'); }}
                      className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 text-xs inline-flex items-center gap-0.5 mt-1"
                    >
                      {t('apiUsage.checkBalanceExternal')} <i className="ri-external-link-line text-[10px]" />
                    </a>
                  </div>
                );
              })()}
              {renderManualBalance('openai')}
            </div>
            {/* Smallest Balance — no public balance API; estimated spend + manual entry only */}
            <div className="border border-slate-100 dark:border-slate-700 rounded-lg p-4">
              <div className="flex items-center justify-between mb-2">
                <span className="text-slate-700 dark:text-slate-300 text-sm font-medium">{t('apiUsage.smallest')}</span>
                <button
                  onClick={() => openEditor('smallest')}
                  title={t('apiUsage.editManualBalance')}
                  className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
                >
                  <i className="ri-pencil-line text-sm" />
                </button>
              </div>
              <div>
                <p className="text-slate-900 dark:text-slate-100 text-xl font-bold tabular-nums inline-flex items-center gap-1.5">
                  {formatCost(providerSpend('smallest'))}
                  <span className="relative group inline-flex items-center">
                    <i className="ri-alert-line text-amber-500 text-sm cursor-help" />
                    <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 bg-slate-900 dark:bg-slate-700 text-white text-[10px] font-normal px-2.5 py-1.5 rounded-lg opacity-0 group-hover:opacity-100 transition-all w-56 text-center pointer-events-none z-10">
                      {t('apiUsage.smallestEstimatedTooltip')}
                    </span>
                  </span>
                </p>
                <p className="text-slate-400 dark:text-slate-500 text-[10px] mt-1">
                  {t('apiUsage.estimatedFromUsage')}
                </p>
                <a
                  href="#"
                  onClick={(e) => { e.preventDefault(); open('https://app.smallest.ai/dashboard'); }}
                  className="text-amber-500 hover:text-amber-600 dark:text-amber-400 dark:hover:text-amber-300 text-xs inline-flex items-center gap-0.5 mt-1"
                >
                  {t('apiUsage.checkBalanceExternal')} <i className="ri-external-link-line text-[10px]" />
                </a>
              </div>
              {renderManualBalance('smallest')}
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

          <div className="overflow-x-auto pb-3">
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
                  <HoverTooltip
                    className="relative w-full flex flex-col items-center justify-end"
                    disabled={total === 0}
                    content={<DayUsageTooltip date={day.date} rows={providerRowsByDate[day.date] ?? []} />}
                  >
                    <div className="w-full flex flex-col items-center justify-end" style={{ height: '110px' }}>
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
                    </div>
                  </HoverTooltip>
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

        {/* Daily tokens by provider */}
        {providerDays.some((d) => d.totalTokens > 0) && (
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
            <div className="flex items-center justify-between mb-4 flex-wrap gap-3">
              <div className="flex items-center gap-2">
                <div className="w-7 h-7 flex items-center justify-center rounded-lg bg-violet-50 dark:bg-violet-500/10 border border-violet-100 dark:border-violet-500/20">
                  <i className="ri-stack-line text-sm text-violet-500" />
                </div>
                <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">Daily tokens by provider</h2>
              </div>
              <div className="flex items-center gap-4 flex-wrap">
                {activeProviders.map((id) => {
                  const m = providerMeta(id);
                  return (
                    <div key={id} className="flex items-center gap-1.5">
                      <span className="w-2.5 h-2.5 rounded-sm" style={{ backgroundColor: m.color }} />
                      <span className="text-slate-500 dark:text-slate-400 text-[10px]">
                        {m.label}
                        {ESTIMATED_TOKEN_PROVIDERS.has(id) ? ' (est.)' : ''}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>

            {hasEstimatedProviders && (
              <p className="text-[10px] text-amber-600 dark:text-amber-400 mb-3 inline-flex items-center gap-1.5">
                <i className="ri-information-line" />
                Deepgram and Smallest are billed by audio seconds and do not return token counts. Their values are estimated at ~{STT_TOKENS_PER_SEC} tokens/sec of audio.
              </p>
            )}

            <div className="overflow-x-auto pb-3 pt-8">
              <div className="flex items-end gap-1.5 min-w-[600px]" style={{ height: '140px' }}>
                {providerDays.map((day) => {
                  const shortDate = day.date.slice(5);
                  return (
                    <div key={day.date} className="flex-1 flex flex-col items-center gap-1">
                      <div className="relative w-full flex flex-col-reverse items-center justify-end" style={{ height: '110px' }}>
                        {day.totalTokens === 0 ? (
                          <div className="w-full bg-slate-100 dark:bg-slate-700 rounded-sm" style={{ height: '4px' }} />
                        ) : (
                          day.entries.map((e) => {
                            const m = providerMeta(e.provider);
                            const pct = (e.tokens / maxProviderDayTokens) * 100;
                            const role = providerRole(e.provider);
                            const roleLabel = role === 'stt' ? 'Speech-to-text' : 'Post-processing';
                            const durationLabel = e.duration > 0
                              ? (e.duration < 60 ? `${Math.round(e.duration)}s` : `${Math.floor(e.duration / 60)}m ${Math.round(e.duration % 60)}s`)
                              : '';
                            return (
                              <div
                                key={e.provider}
                                className="w-full"
                                style={{ height: `${pct}%`, minHeight: '2px' }}
                              >
                                <HoverTooltip
                                  className="w-full h-full"
                                  content={
                                    <div className="text-left min-w-[180px]">
                                      <div className="text-[9px] uppercase tracking-wider text-slate-400 mb-0.5">{roleLabel}</div>
                                      <div className="text-[11px] font-semibold text-slate-100">{m.label}</div>
                                      <div className="text-[10px] tabular-nums text-slate-200 mt-0.5">
                                        {e.tokens.toLocaleString()} tokens{e.estimated ? ' (est.)' : ''}
                                      </div>
                                      {durationLabel && (
                                        <div className="text-[10px] tabular-nums text-slate-400">{durationLabel} audio</div>
                                      )}
                                      <div className="text-[10px] tabular-nums text-slate-400 mt-0.5">{day.date}</div>
                                    </div>
                                  }
                                >
                                  <div className="w-full h-full transition-opacity opacity-90 hover:opacity-100" style={{ backgroundColor: m.color }} />
                                </HoverTooltip>
                              </div>
                            );
                          })
                        )}
                      </div>
                      <span className="text-[9px] text-slate-400 dark:text-slate-500">{shortDate}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        )}

        {/* Audio seconds by provider */}
        {audioByProvider.length > 0 && (
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
            <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700">
              <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">Audio seconds by provider</h2>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">Total audio sent to each provider over the last 31 days.</p>
            </div>
            <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
              {audioByProvider.map((p) => {
                const m = providerMeta(p.provider);
                const pct = (p.seconds / maxAudioSeconds) * 100;
                return (
                  <div key={p.provider} className="px-5 py-3.5 flex flex-col gap-2 group relative">
                    <div className="flex items-center justify-between">
                      <span className="text-slate-800 dark:text-slate-200 text-sm font-medium inline-flex items-center gap-2">
                        <span className="w-2.5 h-2.5 rounded-sm" style={{ backgroundColor: m.color }} />
                        {m.label}
                      </span>
                      <div className="flex items-center gap-4">
                        <span className="text-slate-500 text-xs tabular-nums">{(p.seconds / 60).toFixed(1)} min</span>
                        <span className="text-slate-600 dark:text-slate-300 text-xs font-semibold tabular-nums w-20 text-right">{Math.round(p.seconds).toLocaleString()} s</span>
                      </div>
                    </div>
                    <div className="bg-slate-100 dark:bg-slate-700 rounded-full h-1.5 overflow-hidden">
                      <div className="h-full rounded-full" style={{ width: `${pct}%`, backgroundColor: m.color }} />
                    </div>
                    <div className="absolute left-5 -top-1 bg-slate-900 dark:bg-slate-700 text-white text-[10px] px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-all whitespace-nowrap pointer-events-none z-10">
                      This is for {m.label}.
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

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
