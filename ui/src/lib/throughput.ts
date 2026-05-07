// Rough throughput estimates for STT and LLM models, split by platform.
// Apple Silicon numbers assume Metal acceleration on a base M1/M2 with
// 16 GB unified memory; Pro/Max chips will be faster (we don't currently
// distinguish). Linux numbers assume CPU-only Q4 inference scaled by
// available cores. These are estimates surfaced to the user as a hint —
// not a benchmark — so the goal is "in the right ballpark".

import type { Platform, SystemInfo } from './types';
import type { OllamaCatalogEntry } from './ollama-catalog';

// ── Whisper realtime factor ────────────────────────────────────────────
// Higher = faster. e.g. 5 means 1 sec of audio takes 0.2 sec to transcribe.

const WHISPER_REALTIME_BASE: Record<Platform, Record<string, number>> = {
  apple_silicon: {
    'tiny':       50,
    'tiny.en':    50,
    'base':       30,
    'base.en':    30,
    'small':      14,
    'small.en':   14,
    'medium':     6,
    'medium.en':  6,
    'large-v3':   2.5,
  },
  // Linux CPU-only baseline at 8 cores; we scale linearly with cores below.
  linux: {
    'tiny':       12,
    'tiny.en':    12,
    'base':       7,
    'base.en':    7,
    'small':      3,
    'small.en':   3,
    'medium':     1.4,
    'medium.en':  1.4,
    'large-v3':   0.6,
  },
  other: {
    'tiny':       10,
    'tiny.en':    10,
    'base':       6,
    'base.en':    6,
    'small':      2.5,
    'small.en':   2.5,
    'medium':     1.2,
    'medium.en':  1.2,
    'large-v3':   0.5,
  },
};

export function estimateWhisperRealtime(name: string, sys: SystemInfo): number | null {
  const base = WHISPER_REALTIME_BASE[sys.platform]?.[name];
  if (base == null) return null;
  if (sys.platform === 'linux' || sys.platform === 'other') {
    // Scale linearly off an 8-core reference, clamped to [0.4×, 1.6×].
    const scale = Math.min(1.6, Math.max(0.4, sys.cpu_cores / 8));
    return base * scale;
  }
  return base;
}

/** Floor below which an LLM is too slow to recommend for STT post-processing. */
export const LLM_MIN_TOK_S = 20;

// ── LLM tokens/second ──────────────────────────────────────────────────
// Mapped from the catalog's `params` string. Apple Silicon assumes Metal
// + unified memory; Linux assumes CPU-only Q4 at 8 cores then core-scaled.

interface ParamSpeeds { apple: number; linux8: number }

// Keyed by params string used in OllamaCatalogEntry.
const PARAM_SPEEDS: Record<string, ParamSpeeds> = {
  '360M': { apple: 150, linux8: 60 },
  '0.5B': { apple: 120, linux8: 50 },
  '1B':   { apple: 80,  linux8: 28 },
  '1.5B': { apple: 60,  linux8: 20 },
  '1.7B': { apple: 55,  linux8: 18 },
  '3B':   { apple: 30,  linux8: 9 },
  '3.8B': { apple: 24,  linux8: 7 },
  '4B':   { apple: 22,  linux8: 6 },
  '7B':   { apple: 14,  linux8: 3.5 },
  '8B':   { apple: 12,  linux8: 3 },
};

/**
 * Estimate sustained tok/s for an LLM. If `concurrentLoadMb` > 0 (i.e. Whisper
 * is running concurrently), apply a memory-bandwidth contention penalty.
 */
export function estimateLlmTokensPerSec(
  entry: OllamaCatalogEntry,
  sys: SystemInfo,
  concurrentLoadMb: number,
): number | null {
  const speeds = PARAM_SPEEDS[entry.params];
  if (!speeds) return null;

  let tps: number;
  if (sys.platform === 'apple_silicon') {
    tps = speeds.apple;
  } else if (sys.platform === 'linux') {
    const scale = Math.min(2.0, Math.max(0.3, sys.cpu_cores / 8));
    tps = speeds.linux8 * scale;
  } else {
    const scale = Math.min(1.5, Math.max(0.3, sys.cpu_cores / 8));
    tps = speeds.linux8 * scale * 0.85;
  }

  // Concurrent Whisper transcription contends for memory bandwidth on
  // Apple Silicon and CPU on Linux — both knock ~25–35% off sustained tok/s.
  if (concurrentLoadMb > 0) tps *= 0.7;

  // RAM pressure: if the model's recommended RAM exceeds free RAM, the
  // kernel pages weights in/out, which tanks throughput. Penalise sharply.
  const free = sys.total_ram_mb - concurrentLoadMb;
  if (free < entry.recommended_ram_mb) {
    const ratio = Math.max(0.2, free / entry.recommended_ram_mb);
    tps *= ratio;
  }

  return tps;
}

export function fmtTokensPerSec(tps: number): string {
  if (tps >= 100) return `~${Math.round(tps)} tok/s`;
  if (tps >= 10)  return `~${Math.round(tps)} tok/s`;
  return `~${tps.toFixed(1)} tok/s`;
}

export function fmtRealtime(rt: number): string {
  if (rt >= 10) return `~${Math.round(rt)}× realtime`;
  if (rt >= 1)  return `~${rt.toFixed(1)}× realtime`;
  // Slower than realtime — show as "below realtime" with a fractional factor.
  return `~${rt.toFixed(2)}× realtime`;
}
