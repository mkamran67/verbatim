import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppSelector } from '@/store/hooks';
import type { Config } from '@/lib/types';

interface Props {
  config: Config;
  update: (fn: (c: Config) => void) => void;
}

const STT_LABELS: Record<string, string> = {
  'whisper-local': 'Whisper (local)',
  'openai': 'OpenAI Whisper',
  'deepgram': 'Deepgram',
  'smallest': 'Smallest AI',
};

const PP_LABELS: Record<string, string> = {
  'openai': 'OpenAI',
  'ollama': 'Ollama (local)',
};

export default function RotationAdvanced({ config, update }: Props) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const enabled = config.rotation.enabled;

  return (
    <div className="mt-2">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 text-slate-700 dark:text-slate-300 hover:text-slate-900 dark:hover:text-slate-100 text-sm font-medium cursor-pointer transition-colors"
      >
        <i className={`ri-arrow-${open ? 'down' : 'right'}-s-line text-base`} />
        {t('settings.advanced')}
      </button>
      {open && (
        <div className={`mt-3 pl-5 border-l-2 border-slate-100 dark:border-slate-700 ${enabled ? '' : 'opacity-50 pointer-events-none'}`}>
          <p className="text-slate-400 dark:text-slate-500 text-xs mb-4">{t('settings.advancedDesc')}</p>
          {!enabled && (
            <p className="text-amber-600 dark:text-amber-400 text-xs mb-3">{t('settings.rotationDisabledHint')}</p>
          )}

          <OrderList
            title={t('settings.sttOrder')}
            description={t('settings.sttOrderDesc')}
            items={config.rotation.stt_order}
            labels={STT_LABELS}
            credCheck={(id) => sttHasCredentials(config, id)}
            onChange={(next) => update((c) => { c.rotation.stt_order = next; })}
          />

          <div className="h-4" />

          <OrderList
            title={t('settings.ppOrder')}
            description={t('settings.ppOrderDesc')}
            items={config.rotation.pp_order}
            labels={PP_LABELS}
            credCheck={(id) => ppHasCredentials(config, id)}
            onChange={(next) => update((c) => { c.rotation.pp_order = next; })}
          />
        </div>
      )}
    </div>
  );
}

function sttHasCredentials(c: Config, id: string): boolean {
  switch (id) {
    case 'whisper-local': return true;
    case 'openai': return !!c.openai.api_key;
    case 'deepgram': return !!c.deepgram.api_key;
    case 'smallest': return !!c.smallest.api_key;
    default: return false;
  }
}
function ppHasCredentials(c: Config, id: string): boolean {
  switch (id) {
    case 'openai': return !!c.openai.api_key;
    case 'ollama': return true;
    default: return false;
  }
}

interface OrderListProps {
  title: string;
  description: string;
  items: string[];
  labels: Record<string, string>;
  credCheck: (id: string) => boolean;
  onChange: (next: string[]) => void;
}

function OrderList({ title, description, items, labels, credCheck, onChange }: OrderListProps) {
  const { t } = useTranslation();
  const statusById = useAppSelector((s) => s.rotation.statusById);

  const move = (idx: number, delta: number) => {
    const next = [...items];
    const target = idx + delta;
    if (target < 0 || target >= next.length) return;
    const [moved] = next.splice(idx, 1);
    next.splice(target, 0, moved);
    onChange(next);
  };

  return (
    <div>
      <p className="text-slate-800 dark:text-slate-200 text-sm font-medium">{title}</p>
      <p className="text-slate-400 dark:text-slate-500 text-xs mb-2">{description}</p>
      <ul className="flex flex-col gap-1.5">
        {items.map((id, idx) => {
          const hasCred = credCheck(id);
          const rotState = statusById[id]?.state ?? 'active';
          const pillKey =
            !hasCred ? 'settings.providerNoKey'
            : rotState === 'exhausted' ? 'settings.providerExhausted'
            : rotState === 'auth_error' ? 'settings.providerAuthError'
            : rotState === 'cooling' ? 'settings.providerCooling'
            : 'settings.providerActive';
          const pillTone =
            !hasCred ? 'bg-slate-100 dark:bg-slate-700 text-slate-500 dark:text-slate-400'
            : rotState === 'exhausted' ? 'bg-red-50 dark:bg-red-900/30 text-red-600 dark:text-red-300'
            : rotState === 'auth_error' ? 'bg-red-50 dark:bg-red-900/30 text-red-600 dark:text-red-300'
            : rotState === 'cooling' ? 'bg-amber-50 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300'
            : 'bg-emerald-50 dark:bg-emerald-900/30 text-emerald-700 dark:text-emerald-300';

          return (
            <li
              key={id}
              className="flex items-center gap-2 bg-slate-50 dark:bg-slate-700/40 border border-slate-100 dark:border-slate-700 rounded-lg px-3 py-2"
            >
              <span className="text-slate-400 text-xs font-mono w-4 text-center select-none">{idx + 1}</span>
              <span className="text-slate-800 dark:text-slate-200 text-sm flex-1">{labels[id] ?? id}</span>
              <span className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${pillTone}`}>{t(pillKey)}</span>
              <div className="flex items-center gap-0.5 ml-1">
                <button
                  type="button"
                  onClick={() => move(idx, -1)}
                  disabled={idx === 0}
                  aria-label={t('settings.moveUp')}
                  className="p-1 rounded text-slate-500 hover:text-slate-700 dark:hover:text-slate-200 disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer"
                >
                  <i className="ri-arrow-up-s-line text-base" />
                </button>
                <button
                  type="button"
                  onClick={() => move(idx, 1)}
                  disabled={idx === items.length - 1}
                  aria-label={t('settings.moveDown')}
                  className="p-1 rounded text-slate-500 hover:text-slate-700 dark:hover:text-slate-200 disabled:opacity-30 disabled:cursor-not-allowed cursor-pointer"
                >
                  <i className="ri-arrow-down-s-line text-base" />
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
