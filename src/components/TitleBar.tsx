import { getCurrentWindow } from '@tauri-apps/api/window';
import { motion, useReducedMotion } from 'motion/react';
import { Activity, Clapperboard } from 'lucide-react';
import { useRecording } from '../contexts/RecordingContext';
import { statusPulseTransition } from '../lib/motion';

export function TitleBar() {
  const appWindow = getCurrentWindow();
  const { isRecording, recordingDuration } = useRecording();
  const reduceMotion = useReducedMotion();

  const handleMinimize = () => {
    appWindow.minimize();
  };

  const handleMaximize = async () => {
    try {
      const isMaximized = await appWindow.isMaximized();
      if (isMaximized) {
        await appWindow.unmaximize();
      } else {
        await appWindow.maximize();
      }
    } catch (e) {
      console.error('Maximize error:', e);
    }
  };

  const handleClose = () => {
    appWindow.close();
  };

  const formatDuration = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div
      data-tauri-drag-region
      className="h-10 border-b border-emerald-300/10 bg-[var(--surface-1)] backdrop-blur-md select-none"
    >
      <div className="flex h-full items-center gap-3 px-3" data-tauri-drag-region>
        <div className="inline-flex items-center gap-2 rounded-md border border-emerald-300/15 bg-emerald-500/10 px-2 py-1">
          <Clapperboard className="h-3.5 w-3.5 text-emerald-300" />
          <span className="text-xs font-semibold uppercase tracking-[0.18em] text-emerald-100">Floorpov</span>
        </div>

        <div className="h-4 w-px bg-emerald-200/15" />

        <div className="text-[11px] uppercase tracking-[0.14em] text-neutral-500">Gameplay Recorder</div>

        {isRecording && (
          <motion.div
            className="ml-2 inline-flex items-center gap-2 rounded-md border border-rose-400/30 bg-rose-500/10 px-2.5 py-1"
            animate={
              reduceMotion
                ? undefined
                : {
                    opacity: [0.85, 1, 0.85],
                  }
            }
            transition={statusPulseTransition}
          >
            <Activity className="h-3.5 w-3.5 text-rose-300" />
            <span className="text-[11px] font-semibold uppercase tracking-[0.14em] text-rose-200">
              REC {formatDuration(recordingDuration)}
            </span>
          </motion.div>
        )}
      </div>
      <div className="flex h-full">
        <button
          onClick={handleMinimize}
          className="w-12 h-full flex items-center justify-center text-neutral-400 hover:bg-white/5 hover:text-neutral-200 transition-colors"
          title="Minimize"
        >
          <svg width="10" height="1" viewBox="0 0 10 1" fill="currentColor">
            <rect width="10" height="1" />
          </svg>
        </button>
        <button
          onClick={handleMaximize}
          className="w-12 h-full flex items-center justify-center text-neutral-400 hover:bg-white/5 hover:text-neutral-200 transition-colors"
          title="Maximize"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        </button>
        <button
          onClick={handleClose}
          className="w-12 h-full flex items-center justify-center text-neutral-400 hover:bg-red-600 hover:text-white transition-colors"
          title="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor">
            <path d="M1 1L9 9M9 1L1 9" strokeWidth="1.2" />
          </svg>
        </button>
      </div>
    </div>
  );
}
