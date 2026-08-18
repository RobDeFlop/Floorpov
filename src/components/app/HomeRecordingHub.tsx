import { Activity, Clapperboard, Timer } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useRecording } from "../../contexts/RecordingContext";
import { MEDIA_SECTION_RESIZE_DELTA } from "../../types/settings";
import { formatTime } from "../../utils/format";
import { VideoPlayer } from "../playback/VideoPlayer";
import { RecordingsList } from "../playback/RecordingsList";

const EMPTY_RECORDINGS_MESSAGE =
  "Start a recording by beginning a raid encounter, a Mythic+ run, or a PvP session. Your recordings will appear here.";

export function HomeRecordingHub() {
  const { isRecording, recordingDuration } = useRecording();
  const [isResizingMedia, setIsResizingMedia] = useState(false);
  const resizeCleanupRef = useRef<(() => void) | null>(null);
  const [mediaSectionHeight, setMediaSectionHeight] = useState(() =>
    typeof window === "undefined" ? 520 : Math.round(window.innerHeight * 0.52),
  );
  const mediaSectionMaxHeight =
    typeof window === "undefined" ? 320 : Math.max(320, Math.round(window.innerHeight * 0.66));

  const clampMediaSectionHeight = (height: number, viewportHeight: number) => {
    const minHeight = 320;
    const maxHeight = Math.max(minHeight, Math.round(viewportHeight * 0.66));
    return Math.min(maxHeight, Math.max(minHeight, height));
  };

  useEffect(() => {
    return () => {
      resizeCleanupRef.current?.();
    };
  }, []);

  useEffect(() => {
    const handleWindowResize = () => {
      setMediaSectionHeight((currentHeight) =>
        clampMediaSectionHeight(currentHeight, window.innerHeight),
      );
    };

    handleWindowResize();
    window.addEventListener("resize", handleWindowResize);
    return () => {
      window.removeEventListener("resize", handleWindowResize);
    };
  }, []);

  const handleMediaResizeStart = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    setIsResizingMedia(true);

    const startY = event.clientY;
    const startHeight = mediaSectionHeight;

    const handlePointerMove = (moveEvent: globalThis.PointerEvent) => {
      const deltaY = moveEvent.clientY - startY;
      const targetHeight = startHeight + deltaY;
      setMediaSectionHeight(clampMediaSectionHeight(targetHeight, window.innerHeight));
    };

    const handlePointerEnd = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerEnd);
      window.removeEventListener("pointercancel", handlePointerEnd);
      resizeCleanupRef.current = null;
      setIsResizingMedia(false);
    };

    resizeCleanupRef.current = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerEnd);
      window.removeEventListener("pointercancel", handlePointerEnd);
      resizeCleanupRef.current = null;
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerEnd);
    window.addEventListener("pointercancel", handlePointerEnd);
  };

  const adjustMediaSectionHeight = (delta: number) => {
    setMediaSectionHeight((currentHeight) => {
      return clampMediaSectionHeight(currentHeight + delta, window.innerHeight);
    });
  };

  const handleMediaResizeKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowUp") {
      event.preventDefault();
      adjustMediaSectionHeight(-MEDIA_SECTION_RESIZE_DELTA);
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      adjustMediaSectionHeight(MEDIA_SECTION_RESIZE_DELTA);
    }
  };

  return (
    <div className={`flex h-full min-h-0 flex-col ${isResizingMedia ? "select-none" : ""}`}>
      <header className="flex shrink-0 flex-wrap items-center justify-between gap-4 border-b border-white/10 bg-(--surface-1) px-4 py-4 md:px-6">
        <div>
          <h1 className="inline-flex items-center gap-2 text-lg font-semibold text-neutral-100">
            <Clapperboard className="h-4 w-4 text-neutral-300" aria-hidden="true" />
            Recording Hub
          </h1>
          <p className="mt-1 text-sm text-neutral-400">
            Review recent recordings and continue where you left off.
          </p>
        </div>
        <div
          className="inline-flex items-center gap-3 rounded-sm border border-white/10 bg-black/20 px-3 py-2"
          role="status"
          aria-live="polite"
        >
          <Activity
            className={`h-4 w-4 ${isRecording ? "text-rose-300" : "text-emerald-300"}`}
            aria-hidden="true"
          />
          <div>
            <span className="block text-[10px] uppercase tracking-[0.12em] text-neutral-500">
              Recording status
            </span>
            <span className="block text-sm font-medium text-neutral-100">
              {isRecording ? "Recording" : "Ready"}
            </span>
          </div>
          <div className="inline-flex items-center gap-1.5 border-l border-white/10 pl-3 text-xs text-neutral-300">
            <Timer className="h-3.5 w-3.5 text-neutral-400" aria-hidden="true" />
            {formatTime(recordingDuration)}
          </div>
        </div>
      </header>

      <section
        className="flex w-full shrink-0 flex-col overflow-hidden"
        style={{ height: mediaSectionHeight }}
      >
        <main className="flex min-h-0 flex-1 items-center justify-center overflow-hidden bg-neutral-950/70">
          <VideoPlayer emptyStateMessage="Select a recording to start playback." />
        </main>
      </section>

      <div
        className={`flex h-3 w-full cursor-row-resize items-center justify-center border-y border-white/10 bg-(--surface-2) focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/45 ${
          isResizingMedia ? "bg-white/10" : "hover:bg-white/5"
        }`}
        onPointerDown={handleMediaResizeStart}
        onKeyDown={handleMediaResizeKeyDown}
        role="separator"
        aria-orientation="horizontal"
        aria-label="Resize media section"
        aria-valuemin={320}
        aria-valuenow={mediaSectionHeight}
        aria-valuemax={mediaSectionMaxHeight}
        aria-valuetext={`${mediaSectionHeight}px`}
        tabIndex={0}
      >
        <div className="h-0.5 w-24 rounded-full bg-white/35" />
      </div>

      <RecordingsList
        title="Recent Recordings"
        description="Your latest sessions across Mythic+, raid, and PvP."
        autoSelectLatest
        showManagementActions={false}
        emptyMessage={EMPTY_RECORDINGS_MESSAGE}
      />
    </div>
  );
}
