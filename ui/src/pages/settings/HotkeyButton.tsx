import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '@/lib/tauri';
import type { Hotkey } from '@/lib/types';

/** Format a hotkey for display. Uses the stored label, falling back to numeric. */
export function formatHotkey(hk: Hotkey | null | undefined): string {
  if (!hk) return 'None';
  if (hk.key === 0 && hk.modifiers.length === 0) return 'None';
  if (hk.label && hk.label.trim()) return hk.label;
  if (hk.modifiers.length === 0) return `Code ${hk.key}`;
  return [...hk.modifiers, hk.key].map((c) => `Code ${c}`).join(' + ');
}

export const EMPTY_HOTKEY: Hotkey = { key: 0, modifiers: [], label: '' };

/** Equality on the matched-binding fields (ignores label). */
export function hotkeysEqual(a: Hotkey, b: Hotkey): boolean {
  if (a.key !== b.key) return false;
  if (a.modifiers.length !== b.modifiers.length) return false;
  const sa = [...a.modifiers].sort();
  const sb = [...b.modifiers].sort();
  return sa.every((v, i) => v === sb[i]);
}

interface HotkeyButtonProps {
  value: Hotkey | null;
  target: 'ptt' | 'handsfree';
  onChange: (value: Hotkey) => void;
}

export default function HotkeyButton({ value, target, onChange }: HotkeyButtonProps) {
  const [listening, setListening] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef(false);

  const stopListening = useCallback(() => {
    setListening(false);
    api.resumeHotkey().catch(() => {});
  }, []);

  const startListening = useCallback(async () => {
    cancelRef.current = false;
    setListening(true);
    api.pauseHotkey().catch(() => {});
    try {
      const captured = await api.captureHotkey(target);
      if (cancelRef.current) return;
      onChange({ key: captured.key, modifiers: captured.modifiers, label: captured.label });
    } catch (err) {
      // Timed out or cancelled — silently revert.
      console.debug('hotkey capture ended', err);
    } finally {
      stopListening();
    }
  }, [target, onChange, stopListening]);

  // Cancel-on-Escape and click-outside.
  useEffect(() => {
    if (!listening) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancelRef.current = true;
        stopListening();
      }
    };
    const handleClick = (e: MouseEvent) => {
      if (buttonRef.current && !buttonRef.current.contains(e.target as Node)) {
        cancelRef.current = true;
        stopListening();
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('mousedown', handleClick);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('mousedown', handleClick);
    };
  }, [listening, stopListening]);

  const displayText = listening ? 'Press a key…' : formatHotkey(value);

  return (
    <button
      ref={buttonRef}
      onClick={() => {
        if (!listening) startListening();
      }}
      className={`
        text-sm rounded-lg px-3 py-2 min-w-[160px] text-left transition-all
        ${listening
          ? 'bg-amber-50 dark:bg-amber-500/10 border-2 border-amber-400 text-amber-700 dark:text-amber-400 font-medium'
          : 'bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 text-slate-700 dark:text-slate-300 hover:border-slate-300 dark:hover:border-slate-500 cursor-pointer'
        }
      `}
    >
      {listening ? (
        <span className="flex items-center gap-2">
          <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse" />
          {displayText}
        </span>
      ) : (
        <span className="flex items-center gap-2">
          <i className="ri-keyboard-line text-slate-400 dark:text-slate-500" />
          {displayText}
        </span>
      )}
    </button>
  );
}
