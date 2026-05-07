// Compatibility metadata for whisper.cpp models. Working-set RAM is the
// approximate footprint while transcribing on CPU (model weights + KV
// cache + scratch), not the on-disk size. Recommended cores reflect the
// point at which whisper.cpp keeps up with real-time dictation.

export interface WhisperMeta {
  /** Approximate working-set RAM (MB) during transcription. */
  working_set_mb: number;
  /** Floor below which transcription is unusably slow. */
  min_cores: number;
  /** Cores for smooth, near-real-time transcription. */
  recommended_cores: number;
}

export const WHISPER_META: Record<string, WhisperMeta> = {
  'tiny':       { working_set_mb: 400,  min_cores: 2, recommended_cores: 4 },
  'tiny.en':    { working_set_mb: 400,  min_cores: 2, recommended_cores: 4 },
  'base':       { working_set_mb: 600,  min_cores: 2, recommended_cores: 4 },
  'base.en':    { working_set_mb: 600,  min_cores: 2, recommended_cores: 4 },
  'small':      { working_set_mb: 1500, min_cores: 4, recommended_cores: 6 },
  'small.en':   { working_set_mb: 1500, min_cores: 4, recommended_cores: 6 },
  'medium':     { working_set_mb: 2800, min_cores: 4, recommended_cores: 8 },
  'medium.en':  { working_set_mb: 2800, min_cores: 4, recommended_cores: 8 },
  'large-v3':   { working_set_mb: 5000, min_cores: 6, recommended_cores: 8 },
};

export function whisperWorkingSetMb(name: string | undefined | null): number {
  if (!name) return 0;
  return WHISPER_META[name]?.working_set_mb ?? 0;
}
