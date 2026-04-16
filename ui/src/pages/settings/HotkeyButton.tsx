import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '@/lib/tauri';

/** Maps browser event.code to evdev KEY_* name */
const CODE_TO_KEY: Record<string, string> = {
  // Modifiers
  ControlRight: 'KEY_RIGHTCTRL',
  ControlLeft: 'KEY_LEFTCTRL',
  AltRight: 'KEY_RIGHTALT',
  AltLeft: 'KEY_LEFTALT',
  ShiftRight: 'KEY_RIGHTSHIFT',
  ShiftLeft: 'KEY_LEFTSHIFT',
  // Function keys
  F1: 'KEY_F1',
  F2: 'KEY_F2',
  F3: 'KEY_F3',
  F4: 'KEY_F4',
  F5: 'KEY_F5',
  F6: 'KEY_F6',
  F7: 'KEY_F7',
  F8: 'KEY_F8',
  F9: 'KEY_F9',
  F10: 'KEY_F10',
  F11: 'KEY_F11',
  F12: 'KEY_F12',
  // Special keys
  CapsLock: 'KEY_CAPSLOCK',
  ScrollLock: 'KEY_SCROLLLOCK',
  Pause: 'KEY_PAUSE',
  Insert: 'KEY_INSERT',
  // Letters
  KeyA: 'KEY_A',
  KeyB: 'KEY_B',
  KeyC: 'KEY_C',
  KeyD: 'KEY_D',
  KeyE: 'KEY_E',
  KeyF: 'KEY_F',
  KeyG: 'KEY_G',
  KeyH: 'KEY_H',
  KeyI: 'KEY_I',
  KeyJ: 'KEY_J',
  KeyK: 'KEY_K',
  KeyL: 'KEY_L',
  KeyM: 'KEY_M',
  KeyN: 'KEY_N',
  KeyO: 'KEY_O',
  KeyP: 'KEY_P',
  KeyQ: 'KEY_Q',
  KeyR: 'KEY_R',
  KeyS: 'KEY_S',
  KeyT: 'KEY_T',
  KeyU: 'KEY_U',
  KeyV: 'KEY_V',
  KeyW: 'KEY_W',
  KeyX: 'KEY_X',
  KeyY: 'KEY_Y',
  KeyZ: 'KEY_Z',
  // Numbers
  Digit0: 'KEY_0',
  Digit1: 'KEY_1',
  Digit2: 'KEY_2',
  Digit3: 'KEY_3',
  Digit4: 'KEY_4',
  Digit5: 'KEY_5',
  Digit6: 'KEY_6',
  Digit7: 'KEY_7',
  Digit8: 'KEY_8',
  Digit9: 'KEY_9',
  // Common keys
  Space: 'KEY_SPACE',
  Tab: 'KEY_TAB',
  Enter: 'KEY_ENTER',
  Backspace: 'KEY_BACKSPACE',
  Delete: 'KEY_DELETE',
  Home: 'KEY_HOME',
  End: 'KEY_END',
  PageUp: 'KEY_PAGEUP',
  PageDown: 'KEY_PAGEDOWN',
  ArrowUp: 'KEY_UP',
  ArrowDown: 'KEY_DOWN',
  ArrowLeft: 'KEY_LEFT',
  ArrowRight: 'KEY_RIGHT',
  Minus: 'KEY_MINUS',
  Equal: 'KEY_EQUAL',
  Comma: 'KEY_COMMA',
  Period: 'KEY_DOT',
  Slash: 'KEY_SLASH',
  Semicolon: 'KEY_SEMICOLON',
  Quote: 'KEY_APOSTROPHE',
  Backquote: 'KEY_GRAVE',
  Backslash: 'KEY_BACKSLASH',
  BracketLeft: 'KEY_LEFTBRACE',
  BracketRight: 'KEY_RIGHTBRACE',
};

/** Modifier codes that can be combined with another key */
const MODIFIER_CODES = new Set([
  'ControlRight', 'ControlLeft',
  'AltRight', 'AltLeft',
  'ShiftRight', 'ShiftLeft',
]);

/** Modifier e.key values (fallback when e.code isn't in MODIFIER_CODES) */
const MODIFIER_KEYS = new Set(['Control', 'Alt', 'Shift', 'Meta']);

/** Check if a keyboard event is a modifier */
function isModifier(e: KeyboardEvent): boolean {
  return MODIFIER_CODES.has(e.code) || MODIFIER_KEYS.has(e.key);
}

/** Resolve an evdev KEY_* name from a keyboard event, with e.key fallback */
function keyFromEvent(e: KeyboardEvent): string | null {
  const fromCode = CODE_TO_KEY[e.code];
  if (fromCode) return fromCode;

  // Fallback: derive KEY_* from e.key for common cases
  const k = e.key;
  if (!k || k === 'Unidentified') return null;

  // Single letter
  if (/^[a-zA-Z]$/.test(k)) return `KEY_${k.toUpperCase()}`;
  // Single digit
  if (/^[0-9]$/.test(k)) return `KEY_${k}`;
  // Function keys
  if (/^F(\d{1,2})$/.test(k)) return `KEY_${k.toUpperCase()}`;

  return null;
}

/** Pretty display name for a single KEY_* value */
const KEY_DISPLAY: Record<string, string> = {
  // Modifiers
  KEY_RIGHTCTRL: 'Right Ctrl',
  KEY_LEFTCTRL: 'Left Ctrl',
  KEY_RIGHTALT: 'Right Alt',
  KEY_LEFTALT: 'Left Alt',
  KEY_RIGHTSHIFT: 'Right Shift',
  KEY_LEFTSHIFT: 'Left Shift',
  // Function keys
  KEY_F1: 'F1',
  KEY_F2: 'F2',
  KEY_F3: 'F3',
  KEY_F4: 'F4',
  KEY_F5: 'F5',
  KEY_F6: 'F6',
  KEY_F7: 'F7',
  KEY_F8: 'F8',
  KEY_F9: 'F9',
  KEY_F10: 'F10',
  KEY_F11: 'F11',
  KEY_F12: 'F12',
  // Special keys
  KEY_CAPSLOCK: 'Caps Lock',
  KEY_SCROLLLOCK: 'Scroll Lock',
  KEY_PAUSE: 'Pause',
  KEY_INSERT: 'Insert',
  // Letters
  KEY_A: 'A', KEY_B: 'B', KEY_C: 'C', KEY_D: 'D', KEY_E: 'E',
  KEY_F: 'F', KEY_G: 'G', KEY_H: 'H', KEY_I: 'I', KEY_J: 'J',
  KEY_K: 'K', KEY_L: 'L', KEY_M: 'M', KEY_N: 'N', KEY_O: 'O',
  KEY_P: 'P', KEY_Q: 'Q', KEY_R: 'R', KEY_S: 'S', KEY_T: 'T',
  KEY_U: 'U', KEY_V: 'V', KEY_W: 'W', KEY_X: 'X', KEY_Y: 'Y',
  KEY_Z: 'Z',
  // Numbers
  KEY_0: '0', KEY_1: '1', KEY_2: '2', KEY_3: '3', KEY_4: '4',
  KEY_5: '5', KEY_6: '6', KEY_7: '7', KEY_8: '8', KEY_9: '9',
  // Common keys
  KEY_SPACE: 'Space',
  KEY_TAB: 'Tab',
  KEY_ENTER: 'Enter',
  KEY_BACKSPACE: 'Backspace',
  KEY_DELETE: 'Delete',
  KEY_HOME: 'Home',
  KEY_END: 'End',
  KEY_PAGEUP: 'Page Up',
  KEY_PAGEDOWN: 'Page Down',
  KEY_UP: 'Up',
  KEY_DOWN: 'Down',
  KEY_LEFT: 'Left',
  KEY_RIGHT: 'Right',
  KEY_MINUS: '-',
  KEY_EQUAL: '=',
  KEY_COMMA: ',',
  KEY_DOT: '.',
  KEY_SLASH: '/',
  KEY_SEMICOLON: ';',
  KEY_APOSTROPHE: "'",
  KEY_GRAVE: '`',
  KEY_BACKSLASH: '\\',
  KEY_LEFTBRACE: '[',
  KEY_RIGHTBRACE: ']',
};

/** Format a hotkey config string (e.g. "KEY_LEFTCTRL+KEY_F1") for display */
export function formatHotkey(value: string): string {
  if (!value) return 'None';
  return value
    .split('+')
    .map((k) => KEY_DISPLAY[k] || k)
    .join(' + ');
}

interface HotkeyButtonProps {
  value: string;
  onChange: (value: string) => void;
}

export default function HotkeyButton({ value, onChange }: HotkeyButtonProps) {
  const [listening, setListening] = useState(false);
  const [heldModifiers, setHeldModifiers] = useState<string[]>([]);
  const heldModifiersRef = useRef<string[]>([]);
  const modifierTimerRef = useRef<number | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const stopListening = useCallback(() => {
    if (modifierTimerRef.current !== null) {
      clearTimeout(modifierTimerRef.current);
      modifierTimerRef.current = null;
    }
    setListening(false);
    setHeldModifiers([]);
    heldModifiersRef.current = [];
    api.resumeHotkey().catch(() => {});
  }, []);

  const startListening = useCallback(() => {
    setListening(true);
    api.pauseHotkey().catch(() => {});
  }, []);

  useEffect(() => {
    if (!listening) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.code === 'Escape' || e.key === 'Escape') {
        stopListening();
        return;
      }

      if (isModifier(e)) {
        // Modifier pressed — hold it (max 2), wait for another key
        const current = heldModifiersRef.current;
        const code = e.code || e.key; // fallback identifier
        if (!current.includes(code) && current.length < 2) {
          const next = [...current, code];
          heldModifiersRef.current = next;
          setHeldModifiers(next);
        }
      } else {
        // Non-modifier key pressed — cancel any pending modifier-only timer
        if (modifierTimerRef.current !== null) {
          clearTimeout(modifierTimerRef.current);
          modifierTimerRef.current = null;
        }

        const keyName = keyFromEvent(e);
        if (!keyName) return; // unsupported key

        // Build the combo
        const mods = heldModifiersRef.current;
        const parts = mods.map((m) => CODE_TO_KEY[m] || keyFromEvent({ code: m, key: m } as KeyboardEvent)).filter(Boolean) as string[];
        parts.push(keyName);
        const combo = parts.join('+');

        // If it matches the current hotkey, dismiss
        if (combo === value) {
          stopListening();
          return;
        }

        onChange(combo);
        stopListening();
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (!isModifier(e)) return;

      const code = e.code || e.key;
      const current = heldModifiersRef.current;
      // Only assign single-modifier-as-hotkey if exactly 1 modifier was held
      if (current.length === 1 && current[0] === code) {
        const keyName = CODE_TO_KEY[code] || keyFromEvent(e);
        if (keyName && keyName !== value) {
          // Debounce: wait 200ms in case a non-modifier keydown follows
          modifierTimerRef.current = window.setTimeout(() => {
            modifierTimerRef.current = null;
            onChange(keyName);
            stopListening();
          }, 200);
        } else {
          stopListening();
        }
      } else {
        // Remove this modifier from held list
        const next = current.filter((m) => m !== code);
        heldModifiersRef.current = next;
        setHeldModifiers(next);
        // If all modifiers released without a non-modifier key, cancel
        if (next.length === 0) {
          stopListening();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('keyup', handleKeyUp, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('keyup', handleKeyUp, true);
    };
  }, [listening, onChange, value, stopListening]);

  // Close on click outside
  useEffect(() => {
    if (!listening) return;
    const handleClick = (e: MouseEvent) => {
      if (buttonRef.current && !buttonRef.current.contains(e.target as Node)) {
        stopListening();
      }
    };
    window.addEventListener('mousedown', handleClick);
    return () => window.removeEventListener('mousedown', handleClick);
  }, [listening, stopListening]);

  const displayText = listening
    ? heldModifiers.length > 0
      ? heldModifiers.map((m) => KEY_DISPLAY[CODE_TO_KEY[m]] || m).join(' + ') + ' + ...'
      : 'Press a key...'
    : formatHotkey(value);

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
