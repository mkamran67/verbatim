import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { CreditBalance } from '../../lib/types';

interface BalanceEntry {
  data: CreditBalance | null;
  loading: boolean;
  error: string | null;
}

interface BalanceState {
  deepgram: BalanceEntry;
  openai: BalanceEntry;
}

const initialEntry: BalanceEntry = { data: null, loading: false, error: null };

const initialState: BalanceState = {
  deepgram: { ...initialEntry },
  openai: { ...initialEntry },
};

export const fetchDeepgramBalance = createAsyncThunk(
  'balance/fetchDeepgram',
  (force: boolean) => api.checkDeepgramBalance(force),
);

export const fetchOpenaiCosts = createAsyncThunk(
  'balance/fetchOpenai',
  (force: boolean) => api.checkOpenaiCosts(force),
);

const balanceSlice = createSlice({
  name: 'balance',
  initialState,
  reducers: {},
  extraReducers: (builder) => {
    builder
      .addCase(fetchDeepgramBalance.pending, (state) => {
        state.deepgram.loading = true;
        state.deepgram.error = null;
      })
      .addCase(fetchDeepgramBalance.fulfilled, (state, action) => {
        state.deepgram.data = action.payload;
        state.deepgram.loading = false;
      })
      .addCase(fetchDeepgramBalance.rejected, (state, action) => {
        state.deepgram.loading = false;
        state.deepgram.error = action.error.message ?? 'Failed to fetch balance';
      })
      .addCase(fetchOpenaiCosts.pending, (state) => {
        state.openai.loading = true;
        state.openai.error = null;
      })
      .addCase(fetchOpenaiCosts.fulfilled, (state, action) => {
        state.openai.data = action.payload;
        state.openai.loading = false;
      })
      .addCase(fetchOpenaiCosts.rejected, (state, action) => {
        state.openai.loading = false;
        state.openai.error = action.error.message ?? 'Failed to fetch costs';
      });
  },
});

export default balanceSlice.reducer;
