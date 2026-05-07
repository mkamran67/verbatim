import { useEffect } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { onSttEvent, onConfigChanged, onModelDownloadProgress, onLlmModelDownloadProgress } from '../lib/tauri';
import { useAppDispatch } from './hooks';
import { fetchConfig } from './slices/configSlice';
import { sttEventReceived } from './slices/sttSlice';
import { fetchWhisperModels, fetchLlmModels, whisperDownloadProgressUpdated, llmDownloadProgressUpdated } from './slices/modelsSlice';
import { fetchStats } from './slices/statsSlice';
import { fetchRecent } from './slices/transcriptionsSlice';
import { fetchDeepgramBalance, fetchOpenaiBalance } from './slices/balanceSlice';

export function useTauriListeners() {
  const dispatch = useAppDispatch();

  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [];

    unlisteners.push(
      onSttEvent((event) => {
        dispatch(sttEventReceived(event));
        if (typeof event === 'object' && 'TranscriptionComplete' in event) {
          dispatch(fetchStats());
          dispatch(fetchRecent(50));
          dispatch(fetchDeepgramBalance(false));
          dispatch(fetchOpenaiBalance(false));
        }
      }),
    );

    unlisteners.push(
      onConfigChanged(() => {
        dispatch(fetchConfig());
        dispatch(fetchWhisperModels());
        dispatch(fetchLlmModels());
      }),
    );

    unlisteners.push(
      onModelDownloadProgress((progress) => {
        dispatch(whisperDownloadProgressUpdated(progress));
        if (progress.done) dispatch(fetchWhisperModels());
      }),
    );

    unlisteners.push(
      onLlmModelDownloadProgress((progress) => {
        dispatch(llmDownloadProgressUpdated(progress));
        if (progress.done) dispatch(fetchLlmModels());
      }),
    );

    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, [dispatch]);
}
