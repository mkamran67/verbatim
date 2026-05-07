// Shared compatibility scorer used by the Whisper download list and the
// Ollama post-processing browser. Considers RAM headroom, CPU cores, the
// host platform (Apple Silicon vs Linux CPU), and any concurrent model
// load (e.g. Whisper running while an LLM is picked).

import type { Platform } from './types';

export type Tier = 'best' | 'fits' | 'tight' | 'too_large';

export interface ScoreInput {
  total_ram_mb: number;
  cpu_cores: number;
  platform: Platform;
  /** Working-set RAM (MB) of other models running concurrently. */
  concurrent_load_mb?: number;
  min_ram_mb: number;
  recommended_ram_mb: number;
  /** Min cores for "smooth" latency at this size on the host platform. */
  recommended_cores: number;
  /** Estimated sustained tok/s on this host (LLM only). When provided
   *  alongside `min_throughput_tok_s`, models below the floor are downgraded. */
  estimated_throughput_tok_s?: number;
  /** Floor below which the model is too slow to recommend. */
  min_throughput_tok_s?: number;
}

export interface ScoreResult {
  tier: Tier;
  /** When set, the tier was downgraded for this reason. */
  reason?: 'cpu' | 'throughput';
  /** Effective RAM (MB) used for scoring, after subtracting concurrent load. */
  effective_ram_mb: number;
}

const TIERS: Tier[] = ['too_large', 'tight', 'fits', 'best'];

function downgrade(t: Tier): Tier {
  const i = TIERS.indexOf(t);
  return TIERS[Math.max(0, i - 1)];
}

export function scoreCompatibility(i: ScoreInput): ScoreResult {
  const load = Math.max(0, i.concurrent_load_mb ?? 0);
  const effective = Math.max(0, i.total_ram_mb - load);

  // Apple Silicon has Metal acceleration + unified memory, so the same model
  // is comfortable at lower RAM headroom than a CPU-only Linux box. The
  // headroom multiplier for "best" is the only knob that differs by platform.
  const bestHeadroom = i.platform === 'apple_silicon' ? 1.5 : 2.0;

  let tier: Tier;
  if (effective >= i.recommended_ram_mb * bestHeadroom) tier = 'best';
  else if (effective >= i.recommended_ram_mb) tier = 'fits';
  else if (effective >= i.min_ram_mb) tier = 'tight';
  else tier = 'too_large';

  // On Apple Silicon, performance cores + the Neural Engine make the CPU
  // count axis effectively a non-issue for these model sizes — skip the
  // downgrade entirely. On Linux/other, undersized CPUs still slow inference.
  let reason: 'cpu' | 'throughput' | undefined;
  if (i.platform !== 'apple_silicon' && (tier === 'best' || tier === 'fits')) {
    const cpuFloor = tier === 'best' ? i.recommended_cores : i.recommended_cores - 2;
    if (i.cpu_cores > 0 && i.cpu_cores < cpuFloor) {
      tier = downgrade(tier);
      reason = 'cpu';
    }
  }

  // Throughput floor: too slow to be a usable post-processor regardless
  // of fit. Cap at `fits` (never `best`) and, if it's already at `fits`
  // and below the floor, drop to `tight`.
  if (
    i.estimated_throughput_tok_s != null &&
    i.min_throughput_tok_s != null &&
    i.estimated_throughput_tok_s < i.min_throughput_tok_s
  ) {
    if (tier === 'best' || tier === 'fits') {
      tier = downgrade(tier);
      reason = 'throughput';
    }
  }

  return { tier, reason, effective_ram_mb: effective };
}
