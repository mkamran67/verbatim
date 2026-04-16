import { createSlice, createAsyncThunk, type PayloadAction } from '@reduxjs/toolkit';
import { api } from '../../lib/tauri';
import type { Theme } from '../../lib/theme';

interface ThemeState {
  value: Theme;
}

const initialState: ThemeState = {
  value: 'system',
};

export const loadThemeFromConfig = createAsyncThunk('theme/loadFromConfig', async () => {
  const config = await api.getConfig();
  return (config.general.theme || 'system') as Theme;
});

const themeSlice = createSlice({
  name: 'theme',
  initialState,
  reducers: {
    themeChanged(state, action: PayloadAction<Theme>) {
      state.value = action.payload;
    },
  },
  extraReducers: (builder) => {
    builder.addCase(loadThemeFromConfig.fulfilled, (state, action) => {
      state.value = action.payload;
    });
  },
});

export const { themeChanged } = themeSlice.actions;
export default themeSlice.reducer;
