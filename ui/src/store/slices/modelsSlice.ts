import { createSlice, createAsyncThunk, type PayloadAction } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { ModelInfo, LlmModelInfo, ModelDownloadProgress } from '../../lib/types';

interface ModelsState {
  whisperModels: ModelInfo[];
  llmModels: LlmModelInfo[];
  downloadProgress: ModelDownloadProgress | null;
  llmDownloadProgress: ModelDownloadProgress | null;
  loading: boolean;
}

const initialState: ModelsState = {
  whisperModels: [],
  llmModels: [],
  downloadProgress: null,
  llmDownloadProgress: null,
  loading: false,
};

export const fetchWhisperModels = createAsyncThunk('models/fetchWhisper', () => api.listModels());
export const fetchLlmModels = createAsyncThunk('models/fetchLlm', () => api.listLlmModels());

const modelsSlice = createSlice({
  name: 'models',
  initialState,
  reducers: {
    whisperDownloadProgressUpdated(state, action: PayloadAction<ModelDownloadProgress>) {
      state.downloadProgress = action.payload.done ? null : action.payload;
    },
    llmDownloadProgressUpdated(state, action: PayloadAction<ModelDownloadProgress>) {
      state.llmDownloadProgress = action.payload.done ? null : action.payload;
    },
  },
  extraReducers: (builder) => {
    builder
      .addCase(fetchWhisperModels.pending, (state) => { state.loading = true; })
      .addCase(fetchWhisperModels.fulfilled, (state, action) => {
        state.whisperModels = action.payload;
        state.loading = false;
      })
      .addCase(fetchWhisperModels.rejected, (state) => { state.loading = false; })
      .addCase(fetchLlmModels.fulfilled, (state, action) => {
        state.llmModels = action.payload;
      });
  },
});

export const { whisperDownloadProgressUpdated, llmDownloadProgressUpdated } = modelsSlice.actions;
export default modelsSlice.reducer;
