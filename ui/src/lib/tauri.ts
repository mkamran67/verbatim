import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
  CreditBalance,
  DailyCostSummary,
  DailyProviderUsage,
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
  OllamaRegistryEntry,
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
  getDailyProviderUsage: (days: number) =>
    invoke<DailyProviderUsage[]>("get_daily_provider_usage", { days }),
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
  captureHotkey: (target: "ptt" | "handsfree") =>
    invoke<{ key: number; modifiers: number[]; label: string }>("capture_hotkey", { target }),
  listAudioDevices: () => invoke<string[]>("list_audio_devices"),
  listOpenWindows: () => invoke<string[]>("list_open_windows"),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (name: string) => invoke<void>("download_model", { name }),
  deleteModel: (name: string) => invoke<void>("delete_model", { name }),
  cancelModelDownload: () => invoke<void>("cancel_model_download"),
  // Ollama (out-of-process LLM for post-processing)
  ollamaDetect: () => invoke<{ reachable: boolean; version: string | null; models: string[] }>("ollama_detect"),
  ollamaInstall: () => invoke<void>("ollama_install"),
  ollamaManagedInstalled: () => invoke<boolean>("ollama_managed_installed"),
  ollamaStart: () => invoke<boolean>("ollama_start"),
  ollamaRestart: () => invoke<void>("ollama_restart"),
  ollamaUninstall: () => invoke<void>("ollama_uninstall"),
  ollamaPullModel: (model: string) => invoke<void>("ollama_pull_model", { model }),
  ollamaListLocal: () => invoke<string[]>("ollama_list_local"),
  ollamaDeleteModel: (model: string) => invoke<void>("ollama_delete_model", { model }),
  ollamaSearchRegistry: (query: string) => invoke<OllamaRegistryEntry[]>("ollama_search_registry", { query }),
  // Back-compat shims (kept to minimize UI churn; map to Ollama under the hood)
  listLlmModels: async (): Promise<LlmModelInfo[]> => {
    try {
      const names = await invoke<string[]>("ollama_list_local");
      return names.map((n) => ({
        id: n,
        display_name: n,
        size_bytes: 0,
        downloaded: true,
        context_length: 0,
      }));
    } catch {
      return [];
    }
  },
  downloadLlmModel: (id: string) => invoke<void>("ollama_pull_model", { model: id }),
  deleteLlmModel: (id: string) => invoke<void>("ollama_delete_model", { model: id }),
  cancelLlmModelDownload: async () => { /* Ollama pull cannot be cancelled via API yet */ },
  getSystemInfo: () => invoke<SystemInfo>("get_system_info"),
  startMicMonitor: () => invoke<void>("start_mic_monitor"),
  stopMicMonitor: () => invoke<void>("stop_mic_monitor"),
  getMicLevel: () => invoke<number>("get_mic_level"),
  checkMacPermissions: () => invoke<MacPermissions | null>("check_macos_permissions"),
  checkLinuxInputPermission: () => invoke<boolean>("check_linux_input_permission"),
  openMacSettings: (pane: string) => invoke<void>("open_macos_settings", { pane }),
  checkDeepgramBalance: (force = false) => invoke<CreditBalance>("check_deepgram_balance", { force }),
  getDailyCostSummary: (days: number) =>
    invoke<DailyCostSummary[]>("get_daily_cost_summary", { days }),
  getCostByProvider: () =>
    invoke<ProviderCostSummary[]>("get_cost_by_provider"),
  checkForUpdate: () => invoke<UpdateInfo>("check_for_update"),
  getDebugInfo: () => invoke<DebugInfo>("get_debug_info"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
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
