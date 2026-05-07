import { configureStore } from '@reduxjs/toolkit';
import configReducer from './slices/configSlice';
import sttReducer from './slices/sttSlice';
import modelsReducer from './slices/modelsSlice';
import themeReducer from './slices/themeSlice';
import statsReducer from './slices/statsSlice';
import transcriptionsReducer from './slices/transcriptionsSlice';
import balanceReducer from './slices/balanceSlice';

export const store = configureStore({
  reducer: {
    config: configReducer,
    stt: sttReducer,
    models: modelsReducer,
    theme: themeReducer,
    stats: statsReducer,
    transcriptions: transcriptionsReducer,
    balance: balanceReducer,
  },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
