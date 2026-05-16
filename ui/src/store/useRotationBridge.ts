import { useEffect } from 'react';
import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { useAppDispatch } from './hooks';
import { api, onProviderStatusChanged, onSttEvent } from '../lib/tauri';
import { fetchRotationStatus, providerStatusChanged } from './slices/rotationSlice';
import { useToast } from '../components/ui/Toast';

/** Active = the configured backend currently in use. Inspected on the running
 *  config so we can attribute SttEvent failures to the right provider. */
async function activeProviders(): Promise<{ stt: string; pp: string }> {
  try {
    const cfg = await api.getConfig();
    return {
      stt: cfg.general.backend,
      pp: cfg.post_processing.provider,
    };
  } catch {
    return { stt: 'whisper-local', pp: 'openai' };
  }
}

async function ensureNotificationPermission(): Promise<boolean> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const res = await requestPermission();
      granted = res === 'granted';
    }
    return granted;
  } catch {
    return false;
  }
}

export function useRotationBridge() {
  const dispatch = useAppDispatch();
  const toast = useToast();

  useEffect(() => {
    // Initial fetch + ask for notification permission once at startup.
    dispatch(fetchRotationStatus());
    ensureNotificationPermission();

    const unlisteners: Promise<UnlistenFn>[] = [];

    unlisteners.push(
      onProviderStatusChanged((evt) => {
        dispatch(providerStatusChanged(evt));

        // In-app toast
        const severity =
          evt.event === 'exhausted' || evt.event === 'auth_error' ? 'error'
          : evt.event === 'recovered' ? 'success'
          : 'warning';
        toast.show({
          title: titleFor(evt.event, evt.provider),
          message: evt.message,
          severity,
        });

        // Native OS notification for exhaustion / failover
        if (evt.event === 'exhausted' || evt.event === 'failed_over' || evt.event === 'auth_error') {
          ensureNotificationPermission().then((granted) => {
            if (granted) {
              try {
                sendNotification({
                  title: titleFor(evt.event, evt.provider),
                  body: evt.message,
                });
              } catch (e) {
                console.warn('sendNotification failed', e);
              }
            }
          });
        }
      }),
    );

    // Reactive rotation: when the STT pipeline reports an error we can attribute
    // to the active cloud backend, hand it to the rotation engine.
    unlisteners.push(
      onSttEvent(async (event) => {
        if (typeof event === 'object' && 'PostProcessorError' in event) {
          const { pp } = await activeProviders();
          if (pp && pp !== 'ollama') {
            const body = event.PostProcessorError ?? '';
            // Best-effort status code extraction from the error string.
            const match = body.match(/\b(4\d\d|5\d\d)\b/);
            const statusCode = match ? Number(match[1]) : undefined;
            api.recordProviderFailure(pp, 'post_processing', statusCode, body).catch(console.error);
          }
        }
        if (typeof event === 'object' && 'TranscriptionError' in event) {
          const { stt } = await activeProviders();
          if (stt && stt !== 'whisper-local') {
            const body = event.TranscriptionError ?? '';
            const match = body.match(/\b(4\d\d|5\d\d)\b/);
            const statusCode = match ? Number(match[1]) : undefined;
            api.recordProviderFailure(stt, 'stt', statusCode, body).catch(console.error);
          }
        }
      }),
    );

    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, [dispatch, toast]);
}

function titleFor(event: string, provider: string): string {
  const pretty = provider.charAt(0).toUpperCase() + provider.slice(1);
  switch (event) {
    case 'exhausted':   return `${pretty} balance exhausted`;
    case 'auth_error':  return `${pretty} credentials rejected`;
    case 'failed_over': return `${pretty} unavailable — rotating`;
    case 'recovered':   return `${pretty} recovered`;
    default:            return `${pretty} status changed`;
  }
}
