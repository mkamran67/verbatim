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
}

const initialEntry: BalanceEntry = { data: null, loading: false, error: null };

const initialState: BalanceState = {
  deepgram: { ...initialEntry },
};

export const fetchDeepgramBalance = createAsyncThunk(
  'balance/fetchDeepgram',
  (force: boolean) => api.checkDeepgramBalance(force),
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
      });
  },
});

export default balanceSlice.reducer;
