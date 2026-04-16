export type AppState = "Idle" | "Recording" | "Processing";

export type SttEvent =
  | { StateChanged: AppState }
  | { TranscriptionComplete: { text: string; duration_secs: number; word_count: number } }
  | { TranscriptionError: string }
  | { BackendReady: string }
  | { PostProcessorError: string }
  | { GpuFallback: string }
  | "PostProcessorLoading"
  | "PostProcessorReady";

export interface Transcription {
  id: string;
  text: string;
  word_count: number;
  char_count: number;
  duration_secs: number;
  backend: string;
  language: string | null;
  created_at: string;
  prompt_tokens: number;
  completion_tokens: number;
  post_processing_error: string | null;
  raw_text: string | null;
  stt_model: string | null;
  pp_model: string | null;
}

export interface DailyTokenUsage {
  date: string;
  prompt_tokens: number;
  completion_tokens: number;
}

export interface DailyWordStats {
  date: string;
  total_words: number;
  total_transcriptions: number;
  total_duration_secs: number;
}

export interface Stats {
  today_words: number;
  today_transcriptions: number;
  week_words: number;
  week_transcriptions: number;
  total_words: number;
  total_transcriptions: number;
  today_tokens: number;
  week_tokens: number;
  total_tokens: number;
  today_cost_usd: number;
  week_cost_usd: number;
  total_cost_usd: number;
}

export interface PasteRule {
  app_class: string;
  paste_command: string;
}

export interface Config {
  general: {
    backend: string;
    language: string;
    clipboard_only: boolean;
    hotkeys: string[];
    theme: string;
    ui_language: string;
    onboarding_complete: boolean;
  };
  whisper: {
    model: string;
    model_dir: string;
    threads: number;
  };
  openai: {
    api_key: string;
    admin_key: string;
    model: string;
  };
  deepgram: {
    api_key: string;
    model: string;
  };
  smallest: {
    api_key: string;
  };
  google: {
    credentials_path: string;
  };
  audio: {
    device: string;
    min_duration: number;
    energy_threshold: number;
    noise_cancellation: boolean;
  };
  input: {
    method: string;
    paste_command: string;
    paste_rules: PasteRule[];
  };
  post_processing: {
    enabled: boolean;
    provider: string;
    model: string;
    prompt: string;
    llm_model: string;
    saved_prompts: { name: string; prompt: string; emoji: string }[];
    default_emoji: string;
  };
  hands_free: {
    enabled: boolean;
    hotkeys: string[];
  };
  llm: {
    model_dir: string;
  };
}

export interface SystemInfo {
  total_ram_mb: number;
  cpu_cores: number;
}

export interface ModelInfo {
  name: string;
  size_bytes: number;
  downloaded: boolean;
}

export interface ModelDownloadProgress {
  model: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
  cancelled: boolean;
  verifying: boolean;
}

export interface LlmModelInfo {
  id: string;
  display_name: string;
  size_bytes: number;
  downloaded: boolean;
  context_length: number;
}

export interface ModelTokenUsage {
  model: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface CreditBalance {
  provider: string;
  amount: number;
  currency: string;
  checked_at: string;
  estimated_usage_since: number;
  from_cache: boolean;
}

export interface DailyCostSummary {
  date: string;
  provider: string;
  total_cost_usd: number;
  total_duration_secs: number;
  total_requests: number;
}

export interface ProviderCostSummary {
  provider: string;
  total_cost_usd: number;
  total_duration_secs: number;
  total_requests: number;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_url: string;
  release_notes: string;
}

export interface MacPermissions {
  accessibility: boolean;
  microphone: boolean;
}

export interface LogFileInfo {
  name: string;
  size_bytes: number;
}

export interface VramInfo {
  used_mb: number;
  total_mb: number;
  gpu_name: string;
}

export interface DebugInfo {
  log_dir: string;
  log_files: LogFileInfo[];
  whisper_models_bytes: number;
  llm_models_bytes: number;
  database_bytes: number;
  logs_bytes: number;
  config_bytes: number;
  process_rss_mb: number;
  total_ram_mb: number;
  vram_info: VramInfo | null;
  amd_vram_info: VramInfo | null;
  gpu_backend: string;
  stt_using_gpu: boolean;
  llm_using_gpu: boolean;
  app_vram_mb: number | null;
}
