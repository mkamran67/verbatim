import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import Layout from '../../components/feature/Layout';
import Greeting from './components/Greeting';
import StatsCards from './components/StatsCards';
import RecentTranscriptions from './components/RecentTranscriptions';
import ActivityChart from './components/ActivityChart';
import { Link } from 'react-router-dom';
import { useAppDispatch, useAppSelector } from '@/store/hooks';
import { saveConfig } from '@/store/slices/configSlice';
import { DEFAULT_PP_PROMPT } from '@/lib/prompts';

import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  DragOverlay,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';

const WIDGET_IDS = ['stats', 'tone', 'charts', 'recent'] as const;
type WidgetId = typeof WIDGET_IDS[number];

const STORAGE_KEY = 'verbatim-dashboard-order';

function loadOrder(): WidgetId[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as string[];
      if (parsed.length === WIDGET_IDS.length && WIDGET_IDS.every((id) => parsed.includes(id))) {
        return parsed as WidgetId[];
      }
    }
  } catch { /* ignore */ }
  return [...WIDGET_IDS];
}

function saveOrder(order: WidgetId[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(order));
}

const WIDGET_LABEL_KEYS: Record<WidgetId, string> = {
  stats: 'home.stats',
  tone: 'home.tone',
  charts: 'home.charts',
  recent: 'home.recent',
};

/* ── Sortable widget wrapper ───────────────────────────────────────── */

function SortableWidget({ id, children }: { id: WidgetId; children: React.ReactNode }) {
  const { t } = useTranslation();
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={`relative group ${isDragging ? 'opacity-40 z-50' : ''}`}
    >
      {/* Drag handle */}
      <div
        {...attributes}
        {...listeners}
        className="absolute top-0 left-0 right-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity z-10"
      >
        <div className="flex items-center gap-1.5 px-2.5 py-0.5 rounded-b-lg bg-slate-100 dark:bg-slate-700 border border-t-0 border-slate-200 dark:border-slate-600 cursor-grab active:cursor-grabbing shadow-sm">
          <i className="ri-draggable text-slate-300 dark:text-slate-600 text-sm" />
          <span className="text-[10px] text-slate-400 dark:text-slate-500 font-medium select-none">{t(WIDGET_LABEL_KEYS[id])}</span>
          <i className="ri-draggable text-slate-300 dark:text-slate-600 text-sm" />
        </div>
      </div>
      {children}
    </div>
  );
}

/* ── Individual widget components ──────────────────────────────────── */

function ToneWidget() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const config = useAppSelector((s) => s.config.data);

  const currentPrompt = config?.post_processing.prompt ?? '';
  const savedPrompts = config?.post_processing.saved_prompts ?? [];
  const defaultEmoji = config?.post_processing.default_emoji || '✏️';

  // Default card + up to 4 saved prompts (5 total max)
  const promptCards = [
    { id: '__default__', name: 'Default', prompt: DEFAULT_PP_PROMPT, emoji: defaultEmoji },
    ...savedPrompts.slice(0, 4).map((p) => ({
      id: p.name, name: p.name, prompt: p.prompt, emoji: p.emoji || '📝',
    })),
  ];

  const activeId = currentPrompt === DEFAULT_PP_PROMPT
    ? '__default__'
    : savedPrompts.find((p) => p.prompt === currentPrompt)?.name ?? null;

  const selectPrompt = (prompt: string) => {
    if (!config) return;
    const next = structuredClone(config);
    next.post_processing.prompt = prompt;
    dispatch(saveConfig(next));
  };

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('home.tone')}</h2>
          <p className="text-slate-400 dark:text-slate-500 text-xs mt-0.5">{t('home.toneDesc')}</p>
        </div>
        <Link
          to="/post-processing"
          className="text-xs text-amber-600 dark:text-amber-400 hover:text-amber-800 dark:hover:text-amber-300 font-medium cursor-pointer"
        >
          {t('home.managePrompts')}
        </Link>
      </div>
      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 gap-2.5">
        {promptCards.map((card) => {
          const isActive = card.id === activeId;
          return (
            <button
              key={card.id}
              onClick={() => selectPrompt(card.prompt)}
              className={`flex flex-col items-center gap-2 px-3 py-4 rounded-xl border-2 transition-all cursor-pointer text-center ${
                isActive
                  ? 'border-amber-400 bg-amber-50 dark:bg-amber-500/10 shadow-sm'
                  : 'border-slate-100 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 hover:border-slate-200 dark:hover:border-slate-600 hover:bg-slate-100 dark:hover:bg-slate-700'
              }`}
            >
              <div className={`w-9 h-9 rounded-lg flex items-center justify-center text-xl ${
                isActive
                  ? 'bg-amber-400/20'
                  : 'bg-slate-200/60 dark:bg-slate-700'
              }`}>
                {card.emoji}
              </div>
              <span className={`text-xs font-medium truncate w-full ${
                isActive ? 'text-amber-700 dark:text-amber-400' : 'text-slate-600 dark:text-slate-400'
              }`}>
                {card.name}
              </span>
              {isActive && (
                <span className="text-[9px] font-semibold text-amber-500 uppercase tracking-wider">{t('home.active')}</span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function ChartsAndActions() {
  const { t } = useTranslation();
  const config = useAppSelector((s) => s.config.data);
  const activeModel = config ? (config.whisper.model || config.general.backend) : '';

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div className="h-full">
        <ActivityChart />
      </div>
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-5 flex flex-col gap-3">
        <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('home.quickActions')}</h2>
        <Link to="/recordings" className="flex items-center gap-3 px-4 py-3 bg-amber-500 hover:bg-amber-600 text-white rounded-lg font-medium text-sm transition-all cursor-pointer whitespace-nowrap">
          <i className="ri-mic-line text-sm" />{t('home.viewRecordings')}
        </Link>
        <Link to="/word-count" className="flex items-center gap-3 px-4 py-3 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-lg font-medium text-sm transition-all cursor-pointer whitespace-nowrap">
          <i className="ri-bar-chart-2-line text-sm" />{t('home.viewWordStats')}
        </Link>
        <Link to="/settings" className="flex items-center gap-3 px-4 py-3 bg-slate-50 dark:bg-slate-700 hover:bg-slate-100 dark:hover:bg-slate-600 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-600 rounded-lg font-medium text-sm transition-all cursor-pointer whitespace-nowrap">
          <i className="ri-settings-3-line text-sm" />{t('home.settings')}
        </Link>
        {activeModel && (
          <div className="mt-2 pt-3 border-t border-slate-100 dark:border-slate-700">
            <p className="text-slate-400 dark:text-slate-500 text-xs mb-2">{t('home.activeModel')}</p>
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-emerald-400" />
              <span className="text-slate-700 dark:text-slate-300 text-xs font-medium">{activeModel}</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/* ── Drag overlay placeholder ──────────────────────────────────────── */

function DragPlaceholder({ id }: { id: WidgetId }) {
  const { t } = useTranslation();
  return (
    <div className="rounded-xl border-2 border-dashed border-amber-400 bg-amber-50/50 dark:bg-amber-500/5 px-5 py-6 flex items-center justify-center">
      <span className="text-amber-600 dark:text-amber-400 text-sm font-medium">{t(WIDGET_LABEL_KEYS[id])}</span>
    </div>
  );
}

/* ── Main page ─────────────────────────────────────────────────────── */

export default function Home() {
  const { t } = useTranslation();
  const [order, setOrder] = useState<WidgetId[]>(loadOrder);
  const [activeId, setActiveId] = useState<WidgetId | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const handleDragStart = (event: DragStartEvent) => {
    setActiveId(event.active.id as WidgetId);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    setActiveId(null);
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    setOrder((prev) => {
      const oldIndex = prev.indexOf(active.id as WidgetId);
      const newIndex = prev.indexOf(over.id as WidgetId);
      const next = arrayMove(prev, oldIndex, newIndex);
      saveOrder(next);
      return next;
    });
  };

  const renderWidget = (id: WidgetId) => {
    switch (id) {
      case 'stats': return <StatsCards />;
      case 'tone': return <ToneWidget />;
      case 'charts': return <ChartsAndActions />;
      case 'recent': return <RecentTranscriptions />;
    }
  };

  return (
    <Layout title={t('home.title')} subtitle={t('home.subtitle')}>
      <div className="max-w-[1200px] mb-4">
        <Greeting />
      </div>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragStart={handleDragStart}
        onDragEnd={handleDragEnd}
      >
        <SortableContext items={order} strategy={verticalListSortingStrategy}>
          <div className="flex flex-col gap-5 max-w-[1200px]">
            {order.map((id) => (
              <SortableWidget key={id} id={id}>
                {renderWidget(id)}
              </SortableWidget>
            ))}
          </div>
        </SortableContext>

        <DragOverlay>
          {activeId ? <DragPlaceholder id={activeId} /> : null}
        </DragOverlay>
      </DndContext>
    </Layout>
  );
}
