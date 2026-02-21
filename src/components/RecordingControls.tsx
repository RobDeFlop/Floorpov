import { Circle, Square } from "lucide-react";
import { useRecording } from "../contexts/RecordingContext";
import { useSettings } from "../contexts/SettingsContext";

export function RecordingControls() {
  const {
    isRecording,
    isPreviewing,
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
    try {
      if (isPreviewing) {
        await stopPreview();
      } else {
        await startPreview();
      }
    } catch (error) {
      console.error("Preview toggle failed:", error);
    }
  };

  const handleRecordingToggle = async () => {
    try {
      if (isRecording) {
        await stopRecording();
      } else {
        await startRecording();
      }
    } catch (error) {
      console.error("Recording toggle failed:", error);
    }
  };

  return (
    <div className="flex items-center gap-3 px-4 py-2 bg-neutral-900 border-t border-neutral-800">
      <button
        onClick={handlePreviewToggle}
        disabled={isRecording}
        className={`px-4 py-2 rounded text-sm font-medium transition-colors ${
          isPreviewing
            ? "bg-orange-600 hover:bg-orange-700 text-white"
            : "bg-neutral-700 hover:bg-neutral-600 text-neutral-200"
        } disabled:opacity-50 disabled:cursor-not-allowed`}
      >
        {isPreviewing ? "Stop Preview" : "Start Preview"}
      </button>

      <button
        onClick={handleRecordingToggle}
        className={`flex items-center gap-2 px-4 py-2 rounded text-sm font-medium transition-colors ${
          isRecording
            ? "bg-red-600 hover:bg-red-700 text-white"
            : "bg-neutral-700 hover:bg-neutral-600 text-neutral-200"
        }`}
      >
        {isRecording ? (
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
      </button>

      {isRecording && (
        <div className="flex items-center gap-2 text-sm">
          <div className="w-2 h-2 bg-red-500 rounded-full animate-pulse" />
          <span className="font-mono text-neutral-300">{formatDuration(recordingDuration)}</span>
        </div>
      )}

      {isPreviewing && !isRecording && (
        <span className="text-xs text-neutral-500">Preview active</span>
      )}

      {!isRecording && settings.markerHotkey !== 'none' && (
        <span className="text-xs text-neutral-500 ml-auto">
          Press <kbd className="px-1.5 py-0.5 bg-neutral-800 border border-neutral-700 rounded text-neutral-300 font-mono">{settings.markerHotkey}</kbd> to add marker
        </span>
      )}
    </div>
  );
}
