import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

type Bucket = 'lateNight' | 'morning' | 'afternoon' | 'evening' | 'night';

function bucketForHour(hour: number): Bucket {
  if (hour < 5) return 'lateNight';
  if (hour < 12) return 'morning';
  if (hour < 17) return 'afternoon';
  if (hour < 22) return 'evening';
  return 'night';
}

const VARIANTS_PER_BUCKET = 2;
const READY_CHANCE = 0.2;

export default function Greeting() {
  const { t } = useTranslation();

  const key = useMemo(() => {
    const bucket = bucketForHour(new Date().getHours());
    if (Math.random() < READY_CHANCE) return 'home.greeting.ready';
    const idx = Math.floor(Math.random() * VARIANTS_PER_BUCKET);
    return `home.greeting.${bucket}.${idx}`;
  }, []);

  return (
    <div className="vb-greeting-in flex items-center gap-2.5">
      <span className="relative flex h-2 w-2">
        <span className="absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-60 animate-ping" />
        <span className="relative inline-flex h-2 w-2 rounded-full bg-amber-500" />
      </span>
      <span className="text-slate-800 dark:text-slate-200 text-sm font-medium tracking-tight">
        {t(key)}
      </span>
    </div>
  );
}
