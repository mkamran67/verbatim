import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { Transcription, ModelTokenUsage, ProviderCostSummary } from '../../lib/types';

interface TranscriptionsState {
  recent: Transcription[];
  searchResults: Transcription[];
  forDate: Transcription[];
  modelUsage: ModelTokenUsage[];
  providerCosts: ProviderCostSummary[];
  loading: boolean;
}

const initialState: TranscriptionsState = {
  recent: [],
  searchResults: [],
  forDate: [],
  modelUsage: [],
  providerCosts: [],
  loading: false,
};

export const fetchRecent = createAsyncThunk('transcriptions/fetchRecent', (limit: number) => api.getRecent(limit));

export const searchHistory = createAsyncThunk(
  'transcriptions/search',
  ({ query, limit, offset }: { query: string; limit: number; offset: number }) =>
    api.searchHistory(query, limit, offset),
);

export const fetchTranscriptionsForDate = createAsyncThunk(
  'transcriptions/fetchForDate',
  (date: string) => api.getTranscriptionsForDate(date),
);

export const fetchModelUsage = createAsyncThunk('transcriptions/fetchModelUsage', () => api.getTokenUsageByModel());
export const fetchProviderCosts = createAsyncThunk('transcriptions/fetchProviderCosts', () => api.getCostByProvider());

export const deleteTranscription = createAsyncThunk('transcriptions/delete', async (id: string) => {
  await api.deleteTranscription(id);
  return id;
});

const transcriptionsSlice = createSlice({
  name: 'transcriptions',
  initialState,
  reducers: {},
  extraReducers: (builder) => {
    builder
      .addCase(fetchRecent.pending, (state) => { state.loading = true; })
      .addCase(fetchRecent.fulfilled, (state, action) => {
        state.recent = action.payload;
        state.loading = false;
      })
      .addCase(fetchRecent.rejected, (state) => { state.loading = false; })
      .addCase(searchHistory.fulfilled, (state, action) => {
        state.searchResults = action.payload;
      })
      .addCase(fetchTranscriptionsForDate.fulfilled, (state, action) => {
        state.forDate = action.payload;
      })
      .addCase(fetchModelUsage.fulfilled, (state, action) => {
        state.modelUsage = action.payload;
      })
      .addCase(fetchProviderCosts.fulfilled, (state, action) => {
        state.providerCosts = action.payload;
      })
      .addCase(deleteTranscription.fulfilled, (state, action) => {
        const id = action.payload;
        state.recent = state.recent.filter((t) => t.id !== id);
        state.searchResults = state.searchResults.filter((t) => t.id !== id);
        state.forDate = state.forDate.filter((t) => t.id !== id);
      });
  },
});

export default transcriptionsSlice.reducer;
