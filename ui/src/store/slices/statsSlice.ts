import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { Stats, DailyTokenUsage, DailyWordStats } from '../../lib/types';

interface StatsState {
  data: Stats | null;
  dailyTokens: DailyTokenUsage[];
  dailyWords: DailyWordStats[];
  loading: boolean;
}

const initialState: StatsState = {
  data: null,
  dailyTokens: [],
  dailyWords: [],
  loading: false,
};

export const fetchStats = createAsyncThunk('stats/fetch', () => api.getStats());
export const fetchDailyTokenUsage = createAsyncThunk('stats/fetchDailyTokens', (days: number) => api.getDailyTokenUsage(days));
export const fetchDailyWordStats = createAsyncThunk('stats/fetchDailyWords', (days: number) => api.getDailyWordStats(days));

const statsSlice = createSlice({
  name: 'stats',
  initialState,
  reducers: {},
  extraReducers: (builder) => {
    builder
      .addCase(fetchStats.pending, (state) => { state.loading = true; })
      .addCase(fetchStats.fulfilled, (state, action) => {
        state.data = action.payload;
        state.loading = false;
      })
      .addCase(fetchStats.rejected, (state) => { state.loading = false; })
      .addCase(fetchDailyTokenUsage.fulfilled, (state, action) => {
        state.dailyTokens = action.payload;
      })
      .addCase(fetchDailyWordStats.fulfilled, (state, action) => {
        state.dailyWords = action.payload;
      });
  },
});

export default statsSlice.reducer;
