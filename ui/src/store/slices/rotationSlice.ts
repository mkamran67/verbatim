import { createSlice, createAsyncThunk, type PayloadAction } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { ProviderStatus, ProviderStatusChanged } from '../../lib/types';

interface RotationState {
  statusById: Record<string, { state: string; message?: string }>;
  recentEvents: ProviderStatusChanged[];
}

const initialState: RotationState = {
  statusById: {},
  recentEvents: [],
};

export const fetchRotationStatus = createAsyncThunk(
  'rotation/fetch',
  async (): Promise<ProviderStatus[]> => api.getRotationStatus(),
);

const slice = createSlice({
  name: 'rotation',
  initialState,
  reducers: {
    providerStatusChanged(state, action: PayloadAction<ProviderStatusChanged>) {
      const evt = action.payload;
      // Update status map
      let stateLabel = 'active';
      if (evt.event === 'exhausted') stateLabel = 'exhausted';
      else if (evt.event === 'auth_error') stateLabel = 'auth_error';
      else if (evt.event === 'failed_over') stateLabel = 'cooling';
      else if (evt.event === 'recovered') stateLabel = 'active';
      state.statusById[evt.provider] = { state: stateLabel, message: evt.message };
      // Keep the last 20 events
      state.recentEvents.unshift(evt);
      state.recentEvents = state.recentEvents.slice(0, 20);
    },
    rotationReset(state) {
      state.statusById = {};
      state.recentEvents = [];
    },
  },
  extraReducers: (builder) => {
    builder.addCase(fetchRotationStatus.fulfilled, (state, action) => {
      for (const s of action.payload) {
        state.statusById[s.provider] = { state: s.state };
      }
    });
  },
});

export const { providerStatusChanged, rotationReset } = slice.actions;
export default slice.reducer;
