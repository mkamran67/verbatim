export function Toggle({ on, onChange, disabled }: { on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      onClick={() => !disabled && onChange(!on)}
      className={`w-10 h-5 rounded-full transition-all relative flex-shrink-0 ${disabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'} ${on ? 'bg-amber-500' : 'bg-slate-200 dark:bg-slate-600'}`}
    >
      <span
        className="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all"
        style={{ left: on ? '22px' : '2px' }}
      />
    </button>
  );
}

export function SettingRow({ label, description, children }: { label: string; description?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between py-4 border-b border-slate-50 dark:border-slate-700 last:border-0">
      <div className="flex-1 pr-8">
        <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{label}</p>
        {description && <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{description}</p>}
      </div>
      {children}
    </div>
  );
}
