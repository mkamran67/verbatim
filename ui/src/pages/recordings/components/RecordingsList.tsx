import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { fetchRecent, searchHistory, deleteTranscription } from '@/store/slices/transcriptionsSlice';
import type { Transcription } from '@/lib/types';

interface ExpandedRowProps {
  item: Transcription;
  onClose: () => void;
  onDelete: (id: string) => void;
}

function ExpandedRow({ item, onClose, onDelete }: ExpandedRowProps) {
  const { t } = useTranslation();
  return (
    <div className="px-5 py-4 bg-slate-50 dark:bg-slate-900/50 border-b border-slate-100 dark:border-slate-700">
      <div className="flex items-start justify-between mb-3">
        <p className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-widest">
          {item.raw_text ? t('recordings.postProcessed') : t('recordings.fullTranscript')}
        </p>
        <button onClick={onClose} className="w-5 h-5 flex items-center justify-center text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 cursor-pointer">
          <i className="ri-close-line text-sm" />
        </button>
      </div>
      <p className="text-slate-600 dark:text-slate-300 text-base leading-relaxed">{item.text}</p>
      {item.raw_text && (
        <div className="mt-3">
          <p className="text-xs font-semibold text-slate-500 dark:text-slate-400 uppercase tracking-widest mb-1">{t('recordings.sttRaw')}</p>
          <p className="text-slate-400 dark:text-slate-500 text-base leading-relaxed italic">{item.raw_text}</p>
        </div>
      )}
      {(item.stt_model || item.pp_model) && (
        <div className="flex flex-wrap gap-2 mt-3">
          {item.stt_model && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded text-xs text-blue-700 dark:text-blue-300">
              <i className="ri-mic-line text-[10px]" />STT: {item.backend}/{item.stt_model}
            </span>
          )}
          {!item.stt_model && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded text-xs text-blue-700 dark:text-blue-300">
              <i className="ri-mic-line text-[10px]" />STT: {item.backend}
            </span>
          )}
          {item.pp_model && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-orange-50 dark:bg-orange-900/20 border border-orange-200 dark:border-orange-800 rounded text-xs text-orange-700 dark:text-orange-300">
              <i className="ri-magic-line text-[10px]" />PP: {item.pp_model}
            </span>
          )}
        </div>
      )}
      {item.post_processing_error && (
        <div className="flex items-center gap-2 mt-2 px-2.5 py-1.5 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-800 rounded-md">
          <i className="ri-alert-line text-amber-500 dark:text-amber-400 text-sm flex-shrink-0" />
          <p className="text-amber-700 dark:text-amber-300 text-xs">{t('recordings.ppFailed', { error: item.post_processing_error })}</p>
        </div>
      )}
      <div className="flex items-center gap-3 mt-3">
        <button
          onClick={() => onDelete(item.id)}
          className="text-xs font-medium text-slate-400 hover:text-red-500 cursor-pointer transition-all whitespace-nowrap"
        >
          <i className="ri-delete-bin-line mr-1" />{t('common.delete')}
        </button>
      </div>
    </div>
  );
}

export default function RecordingsList() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const transcriptions = useAppSelector((s) =>
    s.transcriptions.searchResults.length > 0 ? s.transcriptions.searchResults : s.transcriptions.recent
  );
  const [expanded, setExpanded] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (searchQuery.trim()) {
      dispatch(searchHistory({ query: searchQuery, limit: 50, offset: 0 }));
    } else {
      dispatch(fetchRecent(50));
    }
  }, [searchQuery, dispatch]);

  // Clear selection when transcriptions change
  useEffect(() => {
    setSelected((prev) => {
      const ids = new Set(transcriptions.map((t) => t.id));
      const next = new Set([...prev].filter((id) => ids.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [transcriptions]);

  const [copiedId, setCopiedId] = useState<string | null>(null);

  const handleCopy = useCallback((e: React.MouseEvent, item: Transcription) => {
    e.stopPropagation();
    navigator.clipboard.writeText(item.text).then(() => {
      setCopiedId(item.id);
      setTimeout(() => setCopiedId((prev) => prev === item.id ? null : prev), 1500);
    }).catch(() => {});
  }, []);

  const handleDelete = async (id: string) => {
    dispatch(deleteTranscription(id));
    setExpanded(null);
    setSelected((prev) => { const next = new Set(prev); next.delete(id); return next; });
  };

  const toggleSelect = (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    if (selected.size === transcriptions.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(transcriptions.map((t) => t.id)));
    }
  };

  const deleteSelected = async () => {
    for (const id of selected) {
      dispatch(deleteTranscription(id));
    }
    setSelected(new Set());
    setExpanded(null);
  };

  const allSelected = transcriptions.length > 0 && selected.size === transcriptions.length;

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700">
      <div className="px-5 py-4 border-b border-slate-100 dark:border-slate-700 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('recordings.allRecordings')}</h2>
          {selected.size > 0 && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-slate-500 dark:text-slate-400">{t('recordings.selected', { count: selected.size })}</span>
              <button
                onClick={deleteSelected}
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 cursor-pointer transition-all"
              >
                <i className="ri-delete-bin-line text-xs" />
                {t('common.delete')}
              </button>
            </div>
          )}
        </div>
        <div className="flex items-center gap-2 bg-slate-50 dark:bg-slate-700 border border-slate-200 dark:border-slate-600 rounded-lg px-3 py-1.5">
          <i className="ri-search-line text-slate-400 dark:text-slate-500 text-xs" />
          <input
            type="text"
            placeholder="Search..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="bg-transparent text-xs text-slate-600 dark:text-slate-300 outline-none w-32 placeholder:text-slate-400 dark:placeholder:text-slate-500"
          />
        </div>
      </div>

      {/* Table header + body — scrollable */}
      <div className="overflow-x-auto">
      <div className="min-w-[600px]">
      <div className="grid px-5 py-2.5 bg-slate-50 dark:bg-slate-900/50 border-b border-slate-100 dark:border-slate-700" style={{ gridTemplateColumns: '32px 44px 1fr 100px 70px 70px 90px 8px' }}>
        <button
          onClick={selectAll}
          className="w-5 h-5 flex items-center justify-center cursor-pointer"
          title={allSelected ? t('recordings.deselectAll') : t('recordings.selectAll')}
        >
          <div className={`w-3.5 h-3.5 rounded border-2 flex items-center justify-center transition-all ${
            allSelected
              ? 'bg-amber-500 border-amber-500'
              : selected.size > 0
              ? 'bg-amber-500/50 border-amber-500'
              : 'border-slate-300 dark:border-slate-500'
          }`}>
            {(allSelected || selected.size > 0) && <i className="ri-check-line text-white text-[9px]" />}
          </div>
        </button>
        {['', t('recordings.header.text'), t('recordings.header.date'), t('recordings.header.duration'), t('recordings.header.words'), t('recordings.header.tokens'), ''].map((h, i) => (
          <span key={i} className="text-[10px] font-semibold text-slate-400 dark:text-slate-500 uppercase tracking-widest">{h}</span>
        ))}
      </div>

      {transcriptions.length === 0 ? (
        <div className="px-5 py-8 text-center">
          <p className="text-slate-400 dark:text-slate-500 text-sm">{t('recordings.noResults')}</p>
        </div>
      ) : (
        <div className="divide-y divide-slate-50 dark:divide-slate-700/50">
          {transcriptions.map((item) => {
            const isSelected = selected.has(item.id);
            return (
              <div key={item.id}>
                <div
                  className={`grid px-5 py-3.5 items-center hover:bg-slate-50/50 dark:hover:bg-slate-700/50 cursor-pointer transition-all ${
                    isSelected ? 'bg-amber-50/50 dark:bg-amber-500/5' : ''
                  }`}
                  style={{ gridTemplateColumns: '32px 44px 1fr 100px 70px 70px 90px 8px' }}
                  onClick={() => setExpanded(expanded === item.id ? null : item.id)}
                >
                  <button
                    onClick={(e) => toggleSelect(e, item.id)}
                    className="w-5 h-5 flex items-center justify-center cursor-pointer"
                  >
                    <div className={`w-3.5 h-3.5 rounded border-2 flex items-center justify-center transition-all ${
                      isSelected
                        ? 'bg-amber-500 border-amber-500'
                        : 'border-slate-300 dark:border-slate-500 hover:border-amber-400'
                    }`}>
                      {isSelected && <i className="ri-check-line text-white text-[9px]" />}
                    </div>
                  </button>
                  <button
                    onClick={(e) => handleCopy(e, item)}
                    className="w-7 h-7 flex items-center justify-center rounded-md border border-slate-200 dark:border-slate-600 text-slate-400 dark:text-slate-500 hover:text-blue-500 hover:border-blue-300 dark:hover:text-blue-400 dark:hover:border-blue-500 transition-all cursor-pointer"
                    title={t('recordings.copyText')}
                  >
                    <i className={`${copiedId === item.id ? 'ri-check-line text-emerald-500' : 'ri-file-copy-line'} text-sm`} />
                  </button>
                  <div className="min-w-0 pr-4">
                    <div className="flex items-center gap-1.5">
                      <p className="text-slate-800 dark:text-slate-200 text-base font-medium truncate">{item.text.slice(0, 120)}</p>
                      {item.post_processing_error && (
                        <span
                          className="flex-shrink-0 text-amber-500 dark:text-amber-400"
                          title={item.post_processing_error}
                        >
                          <i className="ri-alert-line text-sm" />
                        </span>
                      )}
                    </div>
                    <p className="text-slate-400 dark:text-slate-500 text-sm mt-0.5">
                      {item.language || 'auto'} · {item.stt_model ? `${item.backend} (${item.stt_model})` : item.backend}
                      {item.pp_model && <span className="text-orange-500/70 dark:text-orange-400/70"> · pp: {item.pp_model}</span>}
                    </p>
                  </div>
                  <span className="text-slate-500 dark:text-slate-400 text-xs">{item.created_at.slice(0, 10)}</span>
                  <span className="text-slate-500 dark:text-slate-400 text-xs tabular-nums">{item.duration_secs.toFixed(1)}s</span>
                  <span className="text-slate-700 dark:text-slate-300 text-xs font-medium tabular-nums">{item.word_count.toLocaleString()}</span>
                  {item.prompt_tokens > 0 || item.completion_tokens > 0 ? (
                    <span className="text-orange-600 dark:text-orange-400 text-xs tabular-nums" title={`In: ${item.prompt_tokens} · Out: ${item.completion_tokens}`}>
                      {item.prompt_tokens} / {item.completion_tokens}
                    </span>
                  ) : (
                    <span className="text-slate-300 dark:text-slate-600 text-xs">—</span>
                  )}
                  <div className="w-5 h-5 flex items-center justify-center text-slate-300 dark:text-slate-600">
                    <i className={`ri-arrow-${expanded === item.id ? 'up' : 'down'}-s-line text-sm`} />
                  </div>
                </div>
                {expanded === item.id && (
                  <ExpandedRow item={item} onClose={() => setExpanded(null)} onDelete={handleDelete} />
                )}
              </div>
            );
          })}
        </div>
      )}
      </div>
      </div>
    </div>
  );
}
