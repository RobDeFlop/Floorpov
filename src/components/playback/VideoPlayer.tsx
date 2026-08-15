import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import {
  AlertTriangle,
  Clapperboard,
  LoaderCircle,
  Maximize,
  Pause,
  Play,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useVideo } from "../../contexts/VideoContext";
import { useRecording } from "../../contexts/RecordingContext";
import { ControlIconButton } from "./ControlIconButton";
import { formatTime } from "../../utils/format";

const PLAYBACK_RATES = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2];

export function VideoPlayer() {
  const {
    videoRef,
    currentTime,
    duration,
    isPlaying,
    isVideoLoading,
    volume,
    playbackRate,
    videoSrc,
    togglePlay,
    setVolume,
    setPlaybackRate,
    seek,
    updateTime,
    updateDuration,
    syncIsPlaying,
    setVideoLoading,
    isFullscreen,
    fullscreenPhase,
    toggleFullscreen,
    exitFullscreen,
  } = useVideo();

  const { isRecording, recordingWarning } = useRecording();

  const inlineSurfaceHostRef = useRef<HTMLDivElement>(null);
  const controlsRef = useRef<HTMLDivElement>(null);
  const speedMenuRef = useRef<HTMLDivElement>(null);
  const [showSpeedMenu, setShowSpeedMenu] = useState(false);
  const [isSeeking, setIsSeeking] = useState(false);
  const [seekValue, setSeekValue] = useState(0);
  const [volumeBeforeMute, setVolumeBeforeMute] = useState(1);
  const [showControls, setShowControls] = useState(true);
  const [inlineSurfaceRect, setInlineSurfaceRect] = useState({ left: 0, top: 0, width: 0, height: 0 });
  const autoExitAttemptedRef = useRef(false);
  const controlsHideTimeoutRef = useRef<number | null>(null);

  const showVideo = Boolean(videoSrc) && !isRecording;
  const displayedSeekValue = Math.min(currentTime, Math.max(duration, 0));
  const beginSeeking = () => {
    setIsSeeking(true);
    setSeekValue(displayedSeekValue);
  };

  const resetControlsHideTimer = useCallback(() => {
    setShowControls(true);

    if (controlsHideTimeoutRef.current !== null) {
      window.clearTimeout(controlsHideTimeoutRef.current);
      controlsHideTimeoutRef.current = null;
    }

    if (isFullscreen) {
      controlsHideTimeoutRef.current = window.setTimeout(() => {
        if (!controlsRef.current?.contains(document.activeElement)) {
          setShowControls(false);
        }
        controlsHideTimeoutRef.current = null;
      }, 3000);
    }
  }, [isFullscreen]);

  const inlineSurfaceStyle: CSSProperties | undefined = isFullscreen
    ? undefined
    : inlineSurfaceRect.width > 0 && inlineSurfaceRect.height > 0
      ? {
          left: `${inlineSurfaceRect.left}px`,
          top: `${inlineSurfaceRect.top}px`,
          width: `${inlineSurfaceRect.width}px`,
          height: `${inlineSurfaceRect.height}px`,
        }
      : { visibility: "hidden" };

  const handleVolumeToggle = () => {
    if (volume === 0) {
      setVolume(volumeBeforeMute > 0 ? volumeBeforeMute : 1);
    } else {
      setVolumeBeforeMute(volume);
      setVolume(0);
    }
  };

  const playerSurfaceClassName = isFullscreen
    ? "fixed inset-0 z-[200] flex items-center justify-center overflow-hidden bg-neutral-950"
    : "fixed z-40 overflow-hidden bg-neutral-950/90";

  useEffect(() => {
    if (!showSpeedMenu) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!speedMenuRef.current?.contains(event.target as Node)) {
        setShowSpeedMenu(false);
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setShowSpeedMenu(false);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleEscape);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleEscape);
    };
  }, [showSpeedMenu]);

  useEffect(() => {
    if (!showVideo) {
      syncIsPlaying(false);
      return;
    }

    const syncPlaybackState = () => {
      const videoElement = videoRef.current;
      if (!videoElement) {
        return;
      }

      syncIsPlaying(!videoElement.paused && !videoElement.ended);
    };

    syncPlaybackState();
    const syncTimeout = window.setTimeout(syncPlaybackState, 0);
    const syncFrame = window.requestAnimationFrame(syncPlaybackState);

    return () => {
      window.clearTimeout(syncTimeout);
      window.cancelAnimationFrame(syncFrame);
    };
  }, [showVideo, syncIsPlaying, videoRef]);

  useEffect(() => {
    if (!isSeeking) {
      setSeekValue(displayedSeekValue);
    }
  }, [displayedSeekValue, isSeeking]);

  useEffect(() => {
    const updateInlineSurfaceRect = () => {
      const hostRect = inlineSurfaceHostRef.current?.getBoundingClientRect();
      if (!hostRect) {
        setInlineSurfaceRect({ left: 0, top: 0, width: 0, height: 0 });
        return;
      }

      const nextRect = {
        left: Math.round(hostRect.left),
        top: Math.round(hostRect.top),
        width: Math.max(0, Math.round(hostRect.width)),
        height: Math.max(0, Math.round(hostRect.height)),
      };

      setInlineSurfaceRect((currentRect) => {
        if (
          currentRect.left === nextRect.left &&
          currentRect.top === nextRect.top &&
          currentRect.width === nextRect.width &&
          currentRect.height === nextRect.height
        ) {
          return currentRect;
        }

        return nextRect;
      });
    };

    updateInlineSurfaceRect();

    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", updateInlineSurfaceRect);
      window.addEventListener("scroll", updateInlineSurfaceRect, true);
      return () => {
        window.removeEventListener("resize", updateInlineSurfaceRect);
        window.removeEventListener("scroll", updateInlineSurfaceRect, true);
      };
    }

    const resizeObserver = new ResizeObserver(() => {
      updateInlineSurfaceRect();
    });

    if (inlineSurfaceHostRef.current) {
      resizeObserver.observe(inlineSurfaceHostRef.current);
    }

    window.addEventListener("resize", updateInlineSurfaceRect);
    window.addEventListener("scroll", updateInlineSurfaceRect, true);
    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", updateInlineSurfaceRect);
      window.removeEventListener("scroll", updateInlineSurfaceRect, true);
    };
  }, []);

  useEffect(() => {
    if (showVideo || fullscreenPhase === "windowed") {
      autoExitAttemptedRef.current = false;
      return;
    }

    if (fullscreenPhase === "fullscreen" && !autoExitAttemptedRef.current) {
      autoExitAttemptedRef.current = true;
      void exitFullscreen();
    }
  }, [exitFullscreen, fullscreenPhase, showVideo]);

  useEffect(() => {
    if (!isFullscreen || !showVideo) {
      setShowControls(true);
      if (controlsHideTimeoutRef.current !== null) {
        window.clearTimeout(controlsHideTimeoutRef.current);
        controlsHideTimeoutRef.current = null;
      }
      return;
    }

    resetControlsHideTimer();
    return () => {
      if (controlsHideTimeoutRef.current !== null) {
        window.clearTimeout(controlsHideTimeoutRef.current);
        controlsHideTimeoutRef.current = null;
      }
    };
  }, [isFullscreen, resetControlsHideTimer, showVideo]);

  useEffect(() => {
    if (!showVideo && !isFullscreen) {
      return;
    }

    const handleKeyboard = (event: KeyboardEvent) => {
      resetControlsHideTimer();

      if (event.key === "Escape" && isFullscreen) {
        event.preventDefault();
        setShowSpeedMenu(false);
        void exitFullscreen();
        return;
      }

      if (!showVideo) {
        return;
      }

      const target = event.target;
      const isTextEntry =
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          ["SELECT", "TEXTAREA"].includes(target.tagName) ||
          (target instanceof HTMLInputElement && target.type !== "range"));

      if ((event.key === "f" || event.key === "F") && !isTextEntry) {
        event.preventDefault();
        void toggleFullscreen();
        return;
      }

      if (
        isTextEntry ||
        (target instanceof HTMLElement && ["BUTTON", "INPUT"].includes(target.tagName))
      ) {
        return;
      }

      switch (event.key) {
        case " ":
          event.preventDefault();
          togglePlay();
          break;
        case "ArrowLeft":
          event.preventDefault();
          seek(Math.max(0, currentTime - 5));
          break;
        case "ArrowRight":
          event.preventDefault();
          seek(Math.min(duration, currentTime + 5));
          break;
        case "m":
        case "M":
          event.preventDefault();
          if (volume === 0) {
            setVolume(volumeBeforeMute > 0 ? volumeBeforeMute : 1);
          } else {
            setVolumeBeforeMute(volume);
            setVolume(0);
          }
          break;
      }
    };

    window.addEventListener("keydown", handleKeyboard);
    return () => {
      window.removeEventListener("keydown", handleKeyboard);
    };
  }, [
    currentTime,
    duration,
    exitFullscreen,
    isFullscreen,
    resetControlsHideTimer,
    seek,
    setVolume,
    showVideo,
    toggleFullscreen,
    togglePlay,
    volume,
    volumeBeforeMute,
  ]);

  const playerSurface = (
    <div
      className={playerSurfaceClassName}
      style={inlineSurfaceStyle}
      aria-busy={isVideoLoading}
      onPointerMove={resetControlsHideTimer}
      onPointerDown={resetControlsHideTimer}
    >
      {showVideo && (
        <div className={isFullscreen ? "flex h-full w-full items-center justify-center overflow-hidden" : "h-full w-full"}>
          <video
            ref={videoRef}
            src={videoSrc || undefined}
            className={isFullscreen ? "block h-auto w-auto max-h-full max-w-full object-contain" : "h-full w-full object-contain"}
            controls={false}
            playsInline
            disablePictureInPicture
            preload="metadata"
            onLoadStart={() => {
              setVideoLoading(true);
            }}
            onCanPlay={() => {
              setVideoLoading(false);
            }}
            onError={(event) => {
              setVideoLoading(false);
              const mediaError = event.currentTarget.error;
              console.error("[VideoPlayer] Video load error", {
                code: mediaError?.code,
                message: mediaError?.message,
                networkState: event.currentTarget.networkState,
                readyState: event.currentTarget.readyState,
                src: videoSrc,
              });
            }}
            onTimeUpdate={(e) => updateTime(e.currentTarget.currentTime)}
            onLoadedMetadata={(e) => {
              setVideoLoading(false);
              updateDuration(e.currentTarget.duration);
            }}
            onPlay={() => syncIsPlaying(true)}
            onPause={() => syncIsPlaying(false)}
            onEnded={() => {
              syncIsPlaying(false);
            }}
          />
        </div>
      )}

      {showVideo && isVideoLoading && (
        <div
          className="absolute inset-0 z-10 flex cursor-wait flex-col items-center justify-center gap-2 bg-neutral-950/60 backdrop-blur-sm"
          role="status"
          aria-live="polite"
        >
          <LoaderCircle className="h-6 w-6 animate-spin text-neutral-200" />
          <p className="text-sm font-medium text-neutral-100">Loading recording...</p>
        </div>
      )}

      {isRecording && recordingWarning && (
        <div
          className="absolute left-3 right-3 top-3 z-20 inline-flex items-start gap-2 rounded-sm border border-amber-300/35 bg-amber-500/15 px-3 py-2 text-amber-100"
          role="status"
          aria-live="polite"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <p className="text-xs leading-5">{recordingWarning}</p>
        </div>
      )}

      {!videoSrc && !isRecording && (
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <>
            <div className="mb-3 rounded-full border border-white/20 bg-white/5 p-2">
              <Clapperboard className="h-5 w-5 text-neutral-200" />
            </div>
            <p className="text-neutral-400">No recording loaded</p>
          </>
        </div>
      )}

      {showVideo && (
        <div
          ref={controlsRef}
          className={`absolute bottom-0 left-0 right-0 bg-gradient-to-t from-neutral-950/95 via-neutral-950/70 to-transparent p-3 transition-opacity motion-reduce:transition-none sm:p-4 ${
            showControls ? "visible opacity-100" : "invisible pointer-events-none opacity-0"
          }`}
          aria-hidden={!showControls}
          onPointerEnter={resetControlsHideTimer}
          onFocus={resetControlsHideTimer}
          onBlur={resetControlsHideTimer}
        >
          <div className="flex flex-col gap-3 md:flex-row md:items-center md:gap-3">
            <div className="flex items-center gap-2 sm:gap-3 md:shrink-0">
              <ControlIconButton
                label={isPlaying ? "Pause playback" : "Play recording"}
                onClick={togglePlay}
              >
                {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
              </ControlIconButton>

              <ControlIconButton
                label={volume === 0 ? "Unmute audio" : "Mute audio"}
                onClick={handleVolumeToggle}
              >
                {volume === 0 ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
              </ControlIconButton>

              <div className="flex items-center gap-2">
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  value={volume}
                  onChange={(event) => setVolume(Number(event.target.value))}
                  aria-label="Volume"
                  className="h-2 w-20 accent-emerald-400"
                />
              </div>

              <span className="text-xs font-mono text-white">
                {formatTime(currentTime)} / {formatTime(duration)}
              </span>
            </div>

            <input
              type="range"
              min="0"
              max={Math.max(duration, 0)}
              step="0.01"
              value={isSeeking ? seekValue : displayedSeekValue}
              onChange={(event) => {
                const nextValue = Number(event.target.value);
                setSeekValue(nextValue);
                seek(nextValue);
              }}
              onPointerDown={beginSeeking}
              onPointerUp={() => setIsSeeking(false)}
              onPointerCancel={() => setIsSeeking(false)}
              onFocus={beginSeeking}
              onBlur={() => setIsSeeking(false)}
              aria-label="Timeline"
              aria-valuetext={`${formatTime(isSeeking ? seekValue : currentTime)} of ${formatTime(duration)}`}
              disabled={duration <= 0}
              className="h-2 w-full accent-emerald-400 md:min-w-0 md:flex-1"
            />

            <div className="flex items-center gap-2 md:shrink-0">
              <div ref={speedMenuRef} className="relative">
                <button
                  type="button"
                  onClick={() => setShowSpeedMenu(!showSpeedMenu)}
                  className="rounded border border-neutral-700 bg-neutral-800 px-2 py-1 text-xs text-neutral-100 transition-colors hover:text-neutral-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/45"
                  aria-haspopup="menu"
                  aria-expanded={showSpeedMenu}
                  aria-label="Playback speed"
                >
                  {playbackRate}x
                </button>
                {showSpeedMenu && (
                  <div
                    className="absolute bottom-full left-0 mb-2 rounded border border-neutral-700 bg-neutral-900 py-1 shadow-lg"
                    role="menu"
                    aria-label="Playback speed options"
                  >
                    {PLAYBACK_RATES.map((rate) => (
                      <button
                        key={rate}
                        type="button"
                        onClick={() => {
                          setPlaybackRate(rate);
                          setShowSpeedMenu(false);
                        }}
                        role="menuitemradio"
                        aria-checked={playbackRate === rate}
                        className={`block w-full px-3 py-1 text-left text-xs ${
                          playbackRate === rate
                            ? "bg-white/12 text-neutral-100"
                            : "text-neutral-300 hover:bg-neutral-800"
                        }`}
                      >
                        {rate}x
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <ControlIconButton
                label={isFullscreen ? "Exit fullscreen" : "Toggle fullscreen"}
                onClick={() => void toggleFullscreen()}
              >
                <Maximize className="w-5 h-5" />
              </ControlIconButton>
            </div>
          </div>
        </div>
      )}
    </div>
  );

  return (
    <div ref={inlineSurfaceHostRef} className="relative h-full w-full">
      {createPortal(playerSurface, document.body)}
    </div>
  );
}
