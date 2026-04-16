import { useState, useRef, useEffect } from 'react';

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  className?: string;
  placeholder?: string;
}

export default function Select({ value, onChange, options, className = '', placeholder }: SelectProps) {
  const [open, setOpen] = useState(false);
  const [alignRight, setAlignRight] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handle = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('mousedown', handle);
    return () => window.removeEventListener('mousedown', handle);
  }, [open]);

  // Check if dropdown overflows the viewport and align right if needed
  useEffect(() => {
    if (!open || !dropdownRef.current) return;
    const rect = dropdownRef.current.getBoundingClientRect();
    if (rect.right > window.innerWidth) {
      setAlignRight(true);
    } else {
      setAlignRight(false);
    }
  }, [open]);

  const selected = options.find((o) => o.value === value);
  const displayLabel = !selected ? (placeholder ?? '') : selected.label;

  return (
    <div ref={ref} className={`relative ${className}`}>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={`w-full flex items-center justify-between gap-2 text-sm rounded-lg px-3 py-2 text-left transition-all cursor-pointer border outline-none ${
          placeholder && !selected
            ? 'text-amber-600 dark:text-amber-400 bg-white dark:bg-slate-700 border-amber-200 dark:border-amber-500/30'
            : 'text-slate-700 dark:text-slate-300 bg-slate-50 dark:bg-slate-700 border-slate-200 dark:border-slate-600 hover:border-slate-300 dark:hover:border-slate-500'
        }`}
      >
        <span className="truncate">{displayLabel}</span>
        <i className={`ri-arrow-down-s-line text-sm flex-shrink-0 transition-transform ${open ? 'rotate-180' : ''} ${
          placeholder && !selected ? 'text-amber-500 dark:text-amber-400' : 'text-slate-400 dark:text-slate-500'
        }`} />
      </button>

      {open && (
        <div
          ref={dropdownRef}
          className={`absolute z-50 mt-1 min-w-[160px] max-h-60 overflow-y-auto rounded-lg border border-slate-200 dark:border-slate-600 bg-white dark:bg-slate-700 shadow-lg py-1 ${
            alignRight ? 'right-0' : 'left-0'
          }`}
          style={{ minWidth: ref.current?.offsetWidth }}
        >
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              disabled={opt.disabled}
              onClick={() => {
                if (opt.disabled) return;
                onChange(opt.value);
                setOpen(false);
              }}
              className={`w-full text-left px-3 py-1.5 text-sm transition-colors ${
                opt.disabled
                  ? 'text-slate-400 dark:text-slate-500 cursor-not-allowed'
                  : opt.value === value
                  ? 'bg-amber-50 dark:bg-amber-500/10 text-amber-700 dark:text-amber-400 font-medium cursor-pointer'
                  : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-600 cursor-pointer'
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
