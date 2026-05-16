import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';

export type ToastSeverity = 'info' | 'warning' | 'error' | 'success';

export interface ToastInput {
  title: string;
  message?: string;
  severity?: ToastSeverity;
  /** ms — default 4500. Pass 0 for sticky. */
  duration?: number;
}

interface Toast extends Required<Omit<ToastInput, 'message'>> {
  id: number;
  message?: string;
}

interface ToastContextValue {
  show: (input: ToastInput) => void;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast called outside ToastProvider');
  return ctx;
}

let nextId = 1;

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: number) => {
    setToasts((cur) => cur.filter((t) => t.id !== id));
    const t = timers.current.get(id);
    if (t) {
      clearTimeout(t);
      timers.current.delete(id);
    }
  }, []);

  const show = useCallback((input: ToastInput) => {
    const id = nextId++;
    const toast: Toast = {
      id,
      title: input.title,
      message: input.message,
      severity: input.severity ?? 'info',
      duration: input.duration ?? 4500,
    };
    setToasts((cur) => [...cur, toast]);
    if (toast.duration > 0) {
      const handle = setTimeout(() => dismiss(id), toast.duration);
      timers.current.set(id, handle);
    }
  }, [dismiss]);

  useEffect(() => {
    return () => {
      timers.current.forEach((h) => clearTimeout(h));
      timers.current.clear();
    };
  }, []);

  return (
    <ToastContext.Provider value={{ show, dismiss }}>
      {children}
      <div className="fixed top-4 right-4 z-[10000] flex flex-col gap-2 pointer-events-none">
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

const severityStyles: Record<ToastSeverity, { ring: string; icon: string; iconClass: string }> = {
  info:    { ring: 'ring-sky-200 dark:ring-sky-700',         icon: 'ri-information-line',   iconClass: 'text-sky-500' },
  warning: { ring: 'ring-amber-300 dark:ring-amber-600',     icon: 'ri-alert-line',         iconClass: 'text-amber-500' },
  error:   { ring: 'ring-red-300 dark:ring-red-600',         icon: 'ri-error-warning-line', iconClass: 'text-red-500' },
  success: { ring: 'ring-emerald-300 dark:ring-emerald-600', icon: 'ri-check-line',         iconClass: 'text-emerald-500' },
};

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: () => void }) {
  const s = severityStyles[toast.severity];
  return (
    <div
      className={`pointer-events-auto min-w-[260px] max-w-[360px] bg-white dark:bg-slate-800 rounded-lg shadow-lg ring-1 ${s.ring} px-3 py-2.5 flex items-start gap-2.5 animate-[fadeIn_120ms_ease-out]`}
      role="status"
    >
      <i className={`${s.icon} ${s.iconClass} text-lg leading-none mt-0.5`} />
      <div className="flex-1 min-w-0">
        <p className="text-slate-900 dark:text-slate-100 text-sm font-medium leading-tight">{toast.title}</p>
        {toast.message && (
          <p className="text-slate-500 dark:text-slate-400 text-xs mt-1 leading-snug">{toast.message}</p>
        )}
      </div>
      <button
        onClick={onDismiss}
        className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 cursor-pointer"
        aria-label="Dismiss"
      >
        <i className="ri-close-line text-base" />
      </button>
    </div>
  );
}
