import { useState, useEffect, useCallback, useRef } from 'react';

/** Map browser modifier flags to enigo-compatible names */
function getModifiers(e: KeyboardEvent): string[] {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push('ctrl');
  if (e.shiftKey) mods.push('shift');
  if (e.altKey) mods.push('alt');
  if (e.metaKey) mods.push('super');
  return mods;
}

/** Check if a key code is a modifier */
function isModifier(code: string): boolean {
  return /^(Control|Shift|Alt|Meta)(Left|Right)$/.test(code);
}

/** Map browser event.key / event.code to the key name expected by enigo_backend */
function mapKey(e: KeyboardEvent): string {
  // Special keys by code
  const CODE_MAP: Record<string, string> = {
    Insert: 'Insert',
    Delete: 'Delete',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Backspace: 'Backspace',
    Tab: 'Tab',
    Enter: 'Return',
    Space: 'Space',
    Escape: 'Escape',
  };

  if (CODE_MAP[e.code]) return CODE_MAP[e.code];

  // Function keys
  const fMatch = e.code.match(/^F(\d+)$/);
  if (fMatch) return e.code; // F1, F2, etc.

  // Single character keys — use the key value
  if (e.key.length === 1) return e.key.toLowerCase();

  return e.code;
}

/** Format a paste command string for display */
export function formatPasteCommand(value: string): string {
  if (!value) return 'None';
  return value.split('+').map((p) => {
    const lower = p.toLowerCase();
    if (lower === 'ctrl') return 'Ctrl';
    if (lower === 'shift') return 'Shift';
    if (lower === 'alt') return 'Alt';
    if (lower === 'super') return 'Super';
    // Capitalize first letter for display
    return p.charAt(0).toUpperCase() + p.slice(1);
  }).join(' + ');
}

interface PasteKeyButtonProps {
  value: string;
  onChange: (value: string) => void;
}

export default function PasteKeyButton({ value, onChange }: PasteKeyButtonProps) {
  const [listening, setListening] = useState(false);
  const [heldMods, setHeldMods] = useState<string[]>([]);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const stopListening = useCallback(() => {
    setListening(false);
    setHeldMods([]);
  }, []);

  useEffect(() => {
    if (!listening) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.code === 'Escape') {
        stopListening();
        return;
      }

      if (isModifier(e.code)) {
        setHeldMods(getModifiers(e));
        return;
      }

      // Non-modifier key pressed — build the command
      const mods = getModifiers(e);
      const key = mapKey(e);
      const parts = [...mods, key];
      const cmd = parts.join('+');

      onChange(cmd);
      stopListening();
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (isModifier(e.code)) {
        setHeldMods(getModifiers(e));
      }
    };

    window.addEventListener('keydown', handleKeyDown, true);
    window.addEventListener('keyup', handleKeyUp, true);
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true);
      window.removeEventListener('keyup', handleKeyUp, true);
    };
  }, [listening, onChange, stopListening]);

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
    ? heldMods.length > 0
      ? heldMods.map((m) => m.charAt(0).toUpperCase() + m.slice(1)).join(' + ') + ' + ...'
      : 'Press keys...'
    : formatPasteCommand(value);

  return (
    <button
      ref={buttonRef}
      onClick={() => {
        if (!listening) setListening(true);
      }}
      className={`
        text-sm rounded-lg px-3 py-2 min-w-[140px] text-left transition-all font-mono
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
