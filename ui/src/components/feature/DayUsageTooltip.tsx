import type { DailyProviderUsage } from '@/lib/types';
import {
  ESTIMATED_TOKEN_PROVIDERS,
  estimatedTokensForProvider,
  providerLabel,
  providerRole,
} from '@/lib/tokenEstimate';

interface Props {
  /** ISO date string (YYYY-MM-DD) — already filtered to one day. */
  date: string;
  /** Per-provider rows for THIS date only. Can be empty. */
  rows: DailyProviderUsage[];
}

function fmtDate(iso: string): string {
  const d = new Date(iso + 'T00:00:00');
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

function fmtDuration(secs: number): string {
  if (secs <= 0) return '';
  if (secs < 60) return `${Math.round(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs - m * 60);
  return s === 0 ? `${m}m` : `${m}m ${s}s`;
}

/// Rich hover tooltip used by the daily token bar charts. Splits providers
/// into STT vs Post-processing so the user can see where each day's tokens
/// actually came from, and surfaces audio seconds + estimated counts for
/// providers (Deepgram, Smallest) that don't report real tokens.
export default function DayUsageTooltip({ date, rows }: Props) {
  const stt = rows.filter((r) => providerRole(r.provider) === 'stt');
  const pp = rows.filter((r) => providerRole(r.provider) === 'pp');

  const totalTokens = rows.reduce((sum, r) => {
    const real = r.prompt_tokens + r.completion_tokens;
    return sum + (real > 0 ? real : estimatedTokensForProvider(r.provider, r.duration_secs));
  }, 0);

  return (
    <div className="text-left">
      <div className="font-semibold text-[11px] mb-1.5 text-slate-100">{fmtDate(date)}</div>

      {rows.length === 0 && (
        <div className="text-slate-400 text-[10px]">No activity</div>
      )}

      {stt.length > 0 && (
        <div className="mb-1.5">
          <div className="text-[9px] uppercase tracking-wider text-slate-400 mb-0.5">Speech-to-text</div>
          {stt.map((r) => {
            const real = r.prompt_tokens + r.completion_tokens;
            const est = real === 0 ? estimatedTokensForProvider(r.provider, r.duration_secs) : 0;
            const tokens = real > 0 ? real : est;
            const isEst = est > 0;
            return (
              <div key={r.provider} className="flex justify-between gap-3 text-[10px]">
                <span className="text-slate-200">
                  {providerLabel(r.provider)}
                  {isEst && <span className="text-amber-300/80 ml-1">(est.)</span>}
                </span>
                <span className="tabular-nums text-slate-100">
                  {tokens.toLocaleString()} tok
                  {r.duration_secs > 0 && (
                    <span className="text-slate-400 ml-1">· {fmtDuration(r.duration_secs)}</span>
                  )}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {pp.length > 0 && (
        <div className="mb-1.5">
          <div className="text-[9px] uppercase tracking-wider text-slate-400 mb-0.5">Post-processing</div>
          {pp.map((r) => (
            <div key={r.provider} className="flex justify-between gap-3 text-[10px]">
              <span className="text-slate-200">{providerLabel(r.provider)}</span>
              <span className="tabular-nums text-slate-100">
                {r.prompt_tokens.toLocaleString()} in
                <span className="text-slate-400"> / </span>
                {r.completion_tokens.toLocaleString()} out
              </span>
            </div>
          ))}
        </div>
      )}

      {totalTokens > 0 && (
        <div className="flex justify-between gap-3 text-[10px] pt-1 border-t border-slate-700/60">
          <span className="text-slate-400">Total</span>
          <span className="tabular-nums font-semibold text-slate-100">{totalTokens.toLocaleString()} tok</span>
        </div>
      )}
    </div>
  );
}

// Re-export utility so consumers don't have to import a separate module.
export { ESTIMATED_TOKEN_PROVIDERS };
