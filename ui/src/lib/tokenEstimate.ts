// Providers (Deepgram, Smallest) bill by audio seconds and don't return real
// token counts. We estimate them at a fixed tokens-per-second rate so they
// show up consistently in token-usage views.

export const STT_TOKENS_PER_SEC = 2.5;
export const ESTIMATED_TOKEN_PROVIDERS = new Set(['deepgram', 'smallest']);

export function estimatedTokensForProvider(provider: string, duration_secs: number): number {
  if (ESTIMATED_TOKEN_PROVIDERS.has(provider) && duration_secs > 0) {
    return Math.round(duration_secs * STT_TOKENS_PER_SEC);
  }
  return 0;
}

export type ProviderRole = 'stt' | 'pp';

// Anything not in the PP set is treated as STT. `openai-stt`, `deepgram`,
// `smallest` write rows for transcription; `openai-postproc` and `ollama`
// write rows for the LLM cleanup pass.
const PP_PROVIDERS = new Set(['openai-postproc', 'ollama']);

export function providerRole(provider: string): ProviderRole {
  return PP_PROVIDERS.has(provider) ? 'pp' : 'stt';
}

export function providerLabel(provider: string): string {
  switch (provider) {
    case 'openai-stt': return 'OpenAI Whisper';
    case 'openai-postproc': return 'OpenAI (post-processing)';
    case 'deepgram': return 'Deepgram';
    case 'smallest': return 'Smallest';
    case 'ollama': return 'Ollama';
    default: return provider;
  }
}
