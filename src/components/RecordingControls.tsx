import { Circle, Eye, LoaderCircle, Radio, Square, Timer } from "lucide-react";
import { motion, useReducedMotion } from 'motion/react';
import { useState } from "react";
import { useRecording } from "../contexts/RecordingContext";
import { useSettings } from "../contexts/SettingsContext";
import { panelVariants, smoothTransition } from '../lib/motion';

export function RecordingControls() {
  const reduceMotion = useReducedMotion();
  const [isPreviewBusy, setIsPreviewBusy] = useState(false);
  const [previewAction, setPreviewAction] = useState<'starting' | 'stopping' | null>(null);
  const [isRecordingBusy, setIsRecordingBusy] = useState(false);
  const [recordingAction, setRecordingAction] = useState<'starting' | 'stopping' | null>(null);
  const {
    isRecording,
    isPreviewing,
    lastError,
    recordingDuration,
    startPreview,
    stopPreview,
    startRecording,
    stopRecording,
  } = useRecording();
  const { settings } = useSettings();

  const formatDuration = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const handlePreviewToggle = async () => {
    if (isPreviewBusy) {
      return;
    }

    setIsPreviewBusy(true);
    const shouldStopPreview = isPreviewing;
    setPreviewAction(shouldStopPreview ? 'stopping' : 'starting');
    try {
      if (shouldStopPreview) {
        await stopPreview();
      } else {
        await startPreview();
      }
    } catch (error) {
      console.error("Preview toggle failed:", error);
    } finally {
      setIsPreviewBusy(false);
      setPreviewAction(null);
    }
  };

  const handleRecordingToggle = async () => {
    if (isRecordingBusy) {
      return;
    }

    setIsRecordingBusy(true);
    const shouldStopRecording = isRecording;
    setRecordingAction(shouldStopRecording ? 'stopping' : 'starting');
    try {
      if (shouldStopRecording) {
        await stopRecording();
      } else {
        await startRecording();
      }
    } catch (error) {
      console.error("Recording toggle failed:", error);
    } finally {
      setIsRecordingBusy(false);
      setRecordingAction(null);
    }
  };

  return (
    <motion.div
      className="flex items-center gap-3 px-4 py-2.5 border-t border-emerald-300/10 bg-[var(--surface-2)]"
      variants={panelVariants}
      initial={reduceMotion ? false : 'initial'}
      animate="animate"
      transition={smoothTransition}
    >
      <motion.button
        onClick={handlePreviewToggle}
        disabled={isRecording || isPreviewBusy}
        className={`flex items-center gap-2 px-4 py-2 rounded-md border text-sm font-medium transition-colors ${
          isPreviewing
            ? "border-emerald-300/35 bg-emerald-500/18 hover:bg-emerald-500/25 text-emerald-100"
            : "border-emerald-300/20 bg-white/5 hover:bg-white/10 text-neutral-200"
        } disabled:opacity-50 disabled:cursor-not-allowed`}
        whileHover={reduceMotion ? undefined : { y: -1 }}
        whileTap={reduceMotion ? undefined : { scale: 0.98 }}
      >
        {previewAction ? (
          <>
            <LoaderCircle className="w-4 h-4 animate-spin" />
            {previewAction === 'stopping' ? 'Stopping...' : 'Starting...'}
          </>
        ) : isPreviewing ? (
          <>
            <Eye className="w-4 h-4" />
            Stop Preview
          </>
        ) : (
          <>
            <Eye className="w-4 h-4" />
            Start Preview
          </>
        )}
      </motion.button>

      <motion.button
        onClick={handleRecordingToggle}
        disabled={isRecordingBusy}
        className={`flex items-center gap-2 px-4 py-2 rounded-md border text-sm font-semibold transition-colors ${
          isRecording
            ? "border-rose-300/40 bg-rose-500/25 hover:bg-rose-500/30 text-rose-50"
            : "border-emerald-300/35 bg-emerald-500/20 hover:bg-emerald-500/28 text-emerald-100"
        } disabled:opacity-50 disabled:cursor-not-allowed`}
        whileHover={reduceMotion ? undefined : { y: -1 }}
        whileTap={reduceMotion ? undefined : { scale: 0.98 }}
      >
        {recordingAction ? (
          <>
            <LoaderCircle className="w-4 h-4 animate-spin" />
            {recordingAction === 'stopping' ? 'Stopping...' : 'Starting...'}
          </>
        ) : isRecording ? (
          <>
            <Square className="w-4 h-4" fill="currentColor" />
            Stop Recording
          </>
        ) : (
          <>
            <Circle className="w-4 h-4" fill="currentColor" />
            Start Recording
          </>
        )}
      </motion.button>

      {isRecording && (
        <div className="flex items-center gap-2 rounded-md border border-emerald-300/20 bg-black/20 px-2.5 py-1.5 text-sm">
          <Radio className="w-3.5 h-3.5 text-rose-300 animate-pulse" />
          <Timer className="w-3.5 h-3.5 text-emerald-200" />
          <span className="font-mono text-emerald-100">{formatDuration(recordingDuration)}</span>
        </div>
      )}

      {isPreviewing && !isRecording && (
        <span className="text-xs text-neutral-400">Preview active</span>
      )}

      {!isRecording && settings.markerHotkey !== 'none' && (
          <span className="ml-auto mr-2 text-xs text-neutral-500">
            Press <kbd className="px-1.5 py-0.5 bg-emerald-500/15 border border-emerald-400/30 rounded text-emerald-200 font-mono">{settings.markerHotkey}</kbd> to add marker
          </span>
        )}

      {lastError && (
        <span className="text-xs text-rose-300">{lastError}</span>
      )}
    </motion.div>
  );
}
