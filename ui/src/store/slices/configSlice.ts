import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { Config } from '../../lib/types';

interface ConfigState {
  data: Config | null;
  loading: boolean;
  saving: boolean;
}

const initialState: ConfigState = {
  data: null,
  loading: false,
  saving: false,
};

export const fetchConfig = createAsyncThunk('config/fetch', () => api.getConfig());

export const saveConfig = createAsyncThunk('config/save', async (config: Config) => {
  await api.saveConfig(config);
  return config;
});

const configSlice = createSlice({
  name: 'config',
  initialState,
  reducers: {},
  extraReducers: (builder) => {
    builder
      .addCase(fetchConfig.pending, (state) => { state.loading = true; })
      .addCase(fetchConfig.fulfilled, (state, action) => {
        state.data = action.payload;
        state.loading = false;
      })
      .addCase(fetchConfig.rejected, (state) => { state.loading = false; })
      .addCase(saveConfig.pending, (state) => { state.saving = true; })
      .addCase(saveConfig.fulfilled, (state, action) => {
        state.data = action.payload;
        state.saving = false;
      })
      .addCase(saveConfig.rejected, (state) => { state.saving = false; });
  },
});

export default configSlice.reducer;
