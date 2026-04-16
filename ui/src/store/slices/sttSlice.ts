import { createSlice, type PayloadAction } from '@reduxjs/toolkit';
import type { AppState, SttEvent } from '../../lib/types';

interface SttState {
  appState: AppState;
  isPaused: boolean;
  ppLoading: boolean;
  lastTranscript: { text: string; wordCount: number; durationSecs: number } | null;
  lastError: string | null;
}

const initialState: SttState = {
  appState: 'Idle',
  isPaused: false,
  ppLoading: false,
  lastTranscript: null,
  lastError: null,
};

const sttSlice = createSlice({
  name: 'stt',
  initialState,
  reducers: {
    sttEventReceived(state, action: PayloadAction<SttEvent>) {
      const event = action.payload;
      if (typeof event === 'string') {
        if (event === 'PostProcessorLoading') state.ppLoading = true;
        if (event === 'PostProcessorReady') state.ppLoading = false;
      } else if ('StateChanged' in event) {
        state.appState = event.StateChanged;
      } else if ('TranscriptionComplete' in event) {
        const t = event.TranscriptionComplete;
        state.lastTranscript = { text: t.text, wordCount: t.word_count, durationSecs: t.duration_secs };
      } else if ('TranscriptionError' in event) {
        state.lastError = event.TranscriptionError;
      } else if ('PostProcessorError' in event) {
        state.lastError = event.PostProcessorError;
      } else if ('GpuFallback' in event) {
        state.lastError = event.GpuFallback;
      }
    },
    dismissError(state) { state.lastError = null; },
    hotkeyPaused(state) { state.isPaused = true; },
    hotkeyResumed(state) { state.isPaused = false; },
  },
});

export const { sttEventReceived, dismissError, hotkeyPaused, hotkeyResumed } = sttSlice.actions;
export default sttSlice.reducer;
