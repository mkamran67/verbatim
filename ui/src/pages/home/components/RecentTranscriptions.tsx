import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppSelector } from '@/store/hooks';
import type { Transcription } from '@/lib/types';

interface DetailModalProps {
  item: Transcription;
  onClose: () => void;
}

function DetailModal({ item, onClose }: DetailModalProps) {
  const { t } = useTranslation();
  return (
    <div className="fixed inset-0 bg-black/30 z-50 flex items-center justify-center p-6" onClick={onClose}>
      <div
        className="bg-white dark:bg-slate-800 rounded-2xl border border-slate-100 dark:border-slate-700 w-full max-w-2xl max-h-[80vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-6 border-b border-slate-100 dark:border-slate-700 flex items-start justify-between gap-4">
          <div>
            <h3 className="text-slate-900 dark:text-slate-100 font-semibold text-base">{t('recent.transcription')}</h3>
            <div className="flex items-center gap-3 mt-1.5">
              <span className="text-slate-400 dark:text-slate-500 text-xs">{item.created_at}</span>
              <span className="text-slate-400 dark:text-slate-500 text-xs">{item.duration_secs.toFixed(1)}s</span>
              <span className="text-slate-400 dark:text-slate-500 text-xs">{t('recent.words', { count: item.word_count })}</span>
              {(item.prompt_tokens > 0 || item.completion_tokens > 0) && (
                <span className="text-orange-400 text-xs">tokens: {item.prompt_tokens} in / {item.completion_tokens} out</span>
              )}
              <span className="text-slate-400 dark:text-slate-500 text-xs">{item.backend}</span>
            </div>
          </div>
          <button
            onClick={onClose}
            className="w-7 h-7 flex items-center justify-center rounded-lg hover:bg-slate-100 dark:hover:bg-slate-700 cursor-pointer"
          >
            <i className="ri-close-line text-slate-500 dark:text-slate-400" />
          </button>
        </div>
        <div className="p-6">
          <p className="text-slate-600 dark:text-slate-300 text-sm leading-relaxed">{item.text}</p>
        </div>
      </div>
    </div>
  );
}

export default function RecentTranscriptions() {
  const { t } = useTranslation();
  const transcriptions = useAppSelector((s) => s.transcriptions.recent).slice(0, 8);
  const [selected, setSelected] = useState<Transcription | null>(null);

  if (transcriptions.length === 0) {
    return (
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-8 text-center">
        <p className="text-slate-400 dark:text-slate-500 text-sm">{t('recent.empty')}</p>
      </div>
    );
  }

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
      <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700 flex items-center justify-between">
        <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('recent.title')}</h2>
        <span className="text-slate-400 dark:text-slate-500 text-xs">{t('recent.shown', { count: transcriptions.length })}</span>
      </div>
      <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
        {transcriptions.map((item) => (
          <div
            key={item.id}
            className="px-5 py-3.5 flex items-center gap-4 hover:bg-slate-50/50 dark:hover:bg-slate-700/50 cursor-pointer transition-all"
            onClick={() => setSelected(item)}
          >
            <div className="w-8 h-8 flex items-center justify-center rounded-lg bg-slate-50 dark:bg-slate-700 border border-slate-100 dark:border-slate-600 flex-shrink-0">
              <i className="ri-file-text-line text-slate-400 dark:text-slate-500 text-sm" />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-slate-800 dark:text-slate-200 text-sm font-medium truncate">{item.text.slice(0, 80)}...</p>
              <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{item.created_at} · {item.language || 'auto'}</p>
            </div>
            <span className="text-slate-400 dark:text-slate-500 text-xs tabular-nums whitespace-nowrap">{item.duration_secs.toFixed(1)}s</span>
            <span className="text-slate-600 dark:text-slate-300 text-xs font-medium tabular-nums whitespace-nowrap w-20 text-right">
              {t('recent.words', { count: item.word_count })}
            </span>
            {(item.prompt_tokens > 0 || item.completion_tokens > 0) && (
              <span className="text-orange-500 dark:text-orange-400 text-[10px] tabular-nums whitespace-nowrap" title={`In: ${item.prompt_tokens} · Out: ${item.completion_tokens}`}>
                {item.prompt_tokens}/{item.completion_tokens} tok
              </span>
            )}
            <div className="w-4 h-4 flex items-center justify-center text-slate-300 dark:text-slate-600">
              <i className="ri-arrow-right-s-line text-sm" />
            </div>
          </div>
        ))}
      </div>
      {selected && <DetailModal item={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}
