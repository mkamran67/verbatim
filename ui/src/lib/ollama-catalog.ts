// Curated catalog of Ollama models surfaced in the post-processing model
// browser. Scope: speech-to-text post-processing — fixing punctuation,
// capitalisation, paragraphing, mishears, and producing tidy formatted
// prose. We deliberately exclude:
//   - Code-specialist models (qwen2.5-coder, codegemma): not useful for
//     prose cleanup; would actively misformat dictation.
//   - Reasoning models with explicit thinking traces (deepseek-r1):
//     leak <think>…</think> blocks into the formatted output unless
//     post-stripped, which we don't currently do.
//   - Very large models (14B+, 70B): inference latency makes them
//     impractical for the "transcribe → format → paste" hot path on
//     consumer hardware.
//
// Sizes and RAM thresholds are approximate (Q4_K_M quant unless noted).

export type OllamaCategory = 'fast' | 'balanced' | 'multilingual' | 'quality';

export interface OllamaCatalogEntry {
  /** Tag passed to `ollama pull` (e.g. "qwen2.5:1.5b"). */
  tag: string;
  /** Human-readable family name. */
  family: string;
  /** Param count, e.g. "1.5B". */
  params: string;
  /** Approximate disk size in MB (download + on-disk). */
  size_mb: number;
  /** Minimum RAM (MB) to run comfortably at small context. Below this, expect swapping or OOM. */
  min_ram_mb: number;
  /** RAM (MB) at which the model runs smoothly at 4–8K context. */
  recommended_ram_mb: number;
  /** Min CPU cores for smooth latency at this size on CPU inference. */
  recommended_cores: number;
  /** One-line use-case description, focused on STT cleanup. */
  description: string;
  /** Categories used by the search filter chips. */
  categories: OllamaCategory[];
  /** Free-form keywords that the search query matches against. */
  keywords: string[];
}

export const OLLAMA_CATALOG: OllamaCatalogEntry[] = [
  // ── Fast / very low RAM (good for older laptops) ───────────────────
  {
    tag: 'smollm2:360m',
    family: 'SmolLM2',
    params: '360M',
    size_mb: 250,
    min_ram_mb: 1024,
    recommended_ram_mb: 2048,
    recommended_cores: 4,
    description: 'Tiny instruction-tuned model. Snappy punctuation/capitalisation fixes on very low-RAM machines.',
    categories: ['fast'],
    keywords: ['smol', 'small', 'fast', 'lightweight', 'huggingface'],
  },
  {
    tag: 'smollm2:1.7b',
    family: 'SmolLM2',
    params: '1.7B',
    size_mb: 1100,
    min_ram_mb: 2048,
    recommended_ram_mb: 4096,
    recommended_cores: 4,
    description: 'Stronger formatting than the 360M variant while still very fast. Solid pick for 4 GB systems.',
    categories: ['fast'],
    keywords: ['smol', 'small', 'lightweight'],
  },
  {
    tag: 'llama3.2:1b',
    family: 'Llama 3.2',
    params: '1B',
    size_mb: 1300,
    min_ram_mb: 2048,
    recommended_ram_mb: 4096,
    recommended_cores: 4,
    description: "Meta's compact instruct model. Reliable English punctuation and paragraph formatting.",
    categories: ['fast'],
    keywords: ['llama', 'meta', 'english'],
  },
  {
    tag: 'gemma3:1b',
    family: 'Gemma 3',
    params: '1B',
    size_mb: 800,
    min_ram_mb: 2048,
    recommended_ram_mb: 4096,
    recommended_cores: 4,
    description: "Google's small Gemma. Concise rewrites; sticks closely to the source text.",
    categories: ['fast'],
    keywords: ['gemma', 'google', 'english'],
  },
  {
    tag: 'qwen2.5:0.5b',
    family: 'Qwen 2.5',
    params: '0.5B',
    size_mb: 400,
    min_ram_mb: 1024,
    recommended_ram_mb: 2048,
    recommended_cores: 4,
    description: 'Smallest Qwen. Surprising multilingual coverage at this size — works for short non-English snippets.',
    categories: ['fast', 'multilingual'],
    keywords: ['qwen', 'multilingual', 'chinese', 'japanese', 'korean'],
  },

  // ── Balanced — recommended sweet spot for STT post-processing ──────
  {
    tag: 'qwen2.5:1.5b',
    family: 'Qwen 2.5',
    params: '1.5B',
    size_mb: 1000,
    min_ram_mb: 2048,
    recommended_ram_mb: 4096,
    recommended_cores: 4,
    description: 'Top default for post-processing. Excellent punctuation, formatting, and multilingual support per byte.',
    categories: ['balanced', 'multilingual'],
    keywords: ['qwen', 'recommended', 'multilingual', 'default'],
  },
  {
    tag: 'qwen2.5:3b',
    family: 'Qwen 2.5',
    params: '3B',
    size_mb: 2000,
    min_ram_mb: 4096,
    recommended_ram_mb: 8192,
    recommended_cores: 6,
    description: 'Noticeably better paragraphing and mishear repair than the 1.5B variant. Strong all-rounder.',
    categories: ['balanced', 'multilingual'],
    keywords: ['qwen', 'recommended', 'multilingual'],
  },
  {
    tag: 'llama3.2:3b',
    family: 'Llama 3.2',
    params: '3B',
    size_mb: 2000,
    min_ram_mb: 4096,
    recommended_ram_mb: 8192,
    recommended_cores: 6,
    description: 'Bigger Llama 3.2 — clearly better English formatting than the 1B. Best for English-only dictation.',
    categories: ['balanced'],
    keywords: ['llama', 'meta', 'english'],
  },
  {
    tag: 'phi3.5:3.8b',
    family: 'Phi 3.5',
    params: '3.8B',
    size_mb: 2200,
    min_ram_mb: 4096,
    recommended_ram_mb: 8192,
    recommended_cores: 6,
    description: "Microsoft's instruction-tuned small model. Tidy formatter that follows system prompts closely.",
    categories: ['balanced'],
    keywords: ['phi', 'microsoft', 'english'],
  },
  {
    tag: 'gemma3:4b',
    family: 'Gemma 3',
    params: '4B',
    size_mb: 2700,
    min_ram_mb: 4096,
    recommended_ram_mb: 8192,
    recommended_cores: 6,
    description: 'Larger Gemma 3. Conservative editor — preserves your wording while fixing punctuation.',
    categories: ['balanced'],
    keywords: ['gemma', 'google', 'english'],
  },

  // ── Multilingual specialists ───────────────────────────────────────
  {
    tag: 'aya-expanse:8b',
    family: 'Aya Expanse',
    params: '8B',
    size_mb: 4900,
    min_ram_mb: 8192,
    recommended_ram_mb: 16384,
    recommended_cores: 8,
    description: "Cohere's multilingual instruct model — strong across 23 languages. Pick this if you dictate in non-English regularly.",
    categories: ['multilingual', 'quality'],
    keywords: ['aya', 'cohere', 'multilingual', 'spanish', 'french', 'german', 'arabic', 'hindi'],
  },
  {
    tag: 'qwen2.5:7b',
    family: 'Qwen 2.5',
    params: '7B',
    size_mb: 4400,
    min_ram_mb: 8192,
    recommended_ram_mb: 16384,
    recommended_cores: 8,
    description: 'Top-tier multilingual + instruction quality at 7B. Great if you mix English with another language.',
    categories: ['multilingual', 'quality'],
    keywords: ['qwen', 'multilingual'],
  },

  // ── Higher quality (English-leaning) — needs ≥ 12 GB RAM ──────────
  {
    tag: 'llama3.1:8b',
    family: 'Llama 3.1',
    params: '8B',
    size_mb: 4700,
    min_ram_mb: 8192,
    recommended_ram_mb: 16384,
    recommended_cores: 8,
    description: "Production-quality English formatter. Best polish if you've got the RAM.",
    categories: ['quality'],
    keywords: ['llama', 'meta', 'english'],
  },
  {
    tag: 'mistral:7b',
    family: 'Mistral',
    params: '7B',
    size_mb: 4100,
    min_ram_mb: 8192,
    recommended_ram_mb: 16384,
    recommended_cores: 8,
    description: 'Battle-tested 7B. Conservative, low-hallucination editor — good for transcribing factual content.',
    categories: ['quality'],
    keywords: ['mistral', 'english'],
  },
];

export function searchCatalog(query: string, category: OllamaCategory | 'all'): OllamaCatalogEntry[] {
  const q = query.trim().toLowerCase();
  return OLLAMA_CATALOG.filter((e) => {
    if (category !== 'all' && !e.categories.includes(category)) return false;
    if (!q) return true;
    const hay = [
      e.tag, e.family, e.params, e.description,
      ...e.categories, ...e.keywords,
    ].join(' ').toLowerCase();
    return hay.includes(q);
  });
}

export function fmtSize(mb: number): string {
  if (mb < 1024) return `${mb} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}
