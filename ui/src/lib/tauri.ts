import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
  CreditBalance,
  DailyCostSummary,
  DailyTokenUsage,
  DailyWordStats,
  DebugInfo,
  ProviderCostSummary,
  Stats,
  SystemInfo,
  Transcription,
  ModelInfo,
  ModelDownloadProgress,
  ModelTokenUsage,
  LlmModelInfo,
  SttEvent,
  MacPermissions,
  UpdateInfo,
} from "./types";

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  getStats: () => invoke<Stats>("get_stats"),
  getDailyWordStats: (days: number) =>
    invoke<DailyWordStats[]>("get_daily_word_stats", { days }),
  getTranscriptionsForDate: (date: string) =>
    invoke<Transcription[]>("get_transcriptions_for_date", { date }),
  getDailyTokenUsage: (days: number) =>
    invoke<DailyTokenUsage[]>("get_daily_token_usage", { days }),
  getTokenUsageByModel: () =>
    invoke<ModelTokenUsage[]>("get_token_usage_by_model"),
  getRecent: (limit: number) =>
    invoke<Transcription[]>("get_recent", { limit }),
  searchHistory: (query: string, limit: number, offset: number) =>
    invoke<Transcription[]>("search_history", { query, limit, offset }),
  deleteTranscription: (id: string) =>
    invoke<void>("delete_transcription", { id }),
  pauseHotkey: () => invoke<void>("pause_hotkey"),
  resumeHotkey: () => invoke<void>("resume_hotkey"),
  listAudioDevices: () => invoke<string[]>("list_audio_devices"),
  listOpenWindows: () => invoke<string[]>("list_open_windows"),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (name: string) => invoke<void>("download_model", { name }),
  deleteModel: (name: string) => invoke<void>("delete_model", { name }),
  cancelModelDownload: () => invoke<void>("cancel_model_download"),
  listLlmModels: () => invoke<LlmModelInfo[]>("list_llm_models"),
  downloadLlmModel: (id: string) => invoke<void>("download_llm_model", { id }),
  deleteLlmModel: (id: string) => invoke<void>("delete_llm_model", { id }),
  cancelLlmModelDownload: () => invoke<void>("cancel_llm_model_download"),
  getSystemInfo: () => invoke<SystemInfo>("get_system_info"),
  startMicMonitor: () => invoke<void>("start_mic_monitor"),
  stopMicMonitor: () => invoke<void>("stop_mic_monitor"),
  getMicLevel: () => invoke<number>("get_mic_level"),
  checkMacPermissions: () => invoke<MacPermissions | null>("check_macos_permissions"),
  openMacSettings: (pane: string) => invoke<void>("open_macos_settings", { pane }),
  checkDeepgramBalance: (force = false) => invoke<CreditBalance>("check_deepgram_balance", { force }),
  checkOpenaiCosts: (force = false) => invoke<CreditBalance>("check_openai_costs", { force }),
  getDailyCostSummary: (days: number) =>
    invoke<DailyCostSummary[]>("get_daily_cost_summary", { days }),
  getCostByProvider: () =>
    invoke<ProviderCostSummary[]>("get_cost_by_provider"),
  checkForUpdate: () => invoke<UpdateInfo>("check_for_update"),
  getDebugInfo: () => invoke<DebugInfo>("get_debug_info"),
};

export function onModelDownloadProgress(
  callback: (event: ModelDownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<ModelDownloadProgress>("model-download-progress", (e) =>
    callback(e.payload)
  );
}

export function onLlmModelDownloadProgress(
  callback: (event: ModelDownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<ModelDownloadProgress>("llm-model-download-progress", (e) =>
    callback(e.payload)
  );
}

export function onSttEvent(
  callback: (event: SttEvent) => void
): Promise<UnlistenFn> {
  return listen<SttEvent>("stt-event", (e) => callback(e.payload));
}

export function onConfigChanged(callback: () => void): Promise<UnlistenFn> {
  return listen<void>("config-changed", () => callback());
}
