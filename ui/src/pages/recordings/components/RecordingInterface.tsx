import { useTranslation } from 'react-i18next';
import { useAppSelector } from '@/store/hooks';

export default function RecordingInterface() {
  const { t } = useTranslation();
  const appState = useAppSelector((s) => s.stt.appState);

  const isRecording = appState === 'Recording';
  const isProcessing = appState === 'Processing';
  const isIdle = appState === 'Idle';

  return (
    <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-100 dark:border-slate-700 p-6">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-slate-900 dark:text-slate-100 font-semibold text-sm">{t('recordingInterface.status')}</h2>
        <div className={`flex items-center gap-2 text-xs font-medium px-3 py-1.5 rounded-full border whitespace-nowrap ${
          isRecording ? 'bg-red-50 dark:bg-red-500/10 border-red-200 dark:border-red-500/30 text-red-600 dark:text-red-400' :
          isProcessing ? 'bg-amber-50 dark:bg-amber-500/10 border-amber-200 dark:border-amber-500/30 text-amber-600 dark:text-amber-400' :
          'bg-emerald-50 dark:bg-emerald-500/10 border-emerald-200 dark:border-emerald-500/30 text-emerald-600 dark:text-emerald-400'
        }`}>
          <span className={`w-1.5 h-1.5 rounded-full ${
            isRecording ? 'bg-red-500 animate-pulse' :
            isProcessing ? 'bg-amber-500 animate-pulse' :
            'bg-emerald-500'
          }`} />
          {isRecording ? t('topbar.recording') : isProcessing ? t('topbar.processing') : t('recordingInterface.idle')}
        </div>
      </div>

      {/* Status display */}
      <div className="flex flex-col items-center gap-5 py-6 border border-slate-100 dark:border-slate-700 rounded-xl bg-slate-50/50 dark:bg-slate-900/50">
        {/* Waveform bars during recording */}
        {isRecording && (
          <div className="flex items-center gap-1 h-10">
            {Array.from({ length: 28 }).map((_, i) => (
              <div
                key={i}
                className="w-1 bg-amber-400 rounded-full animate-pulse"
                style={{
                  height: `${Math.random() * 32 + 8}px`,
                  animationDelay: `${(i * 0.05).toFixed(2)}s`,
                  animationDuration: `${(0.4 + Math.random() * 0.4).toFixed(2)}s`,
                }}
              />
            ))}
          </div>
        )}

        {/* Status icon */}
        <div className="relative">
          <div className={`w-20 h-20 rounded-full flex items-center justify-center ${
            isRecording ? 'bg-red-500' :
            isProcessing ? 'bg-amber-400' :
            'bg-slate-900 dark:bg-slate-700'
          }`}>
            <i className={`text-white text-2xl ${
              isRecording ? 'ri-mic-line' :
              isProcessing ? 'ri-loader-4-line animate-spin' :
              'ri-mic-line'
            }`} />
          </div>
          {isRecording && (
            <span className="absolute -inset-2 rounded-full border-2 border-red-300 animate-ping" />
          )}
        </div>

        <p className="text-slate-400 dark:text-slate-500 text-xs text-center">
          {isIdle ? t('recordingInterface.pressHotkey') :
           isRecording ? t('recordingInterface.releaseHotkey') :
           t('recordingInterface.processingAudio')}
        </p>
      </div>
    </div>
  );
}
