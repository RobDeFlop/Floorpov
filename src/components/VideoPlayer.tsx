import { useRef, useState } from "react";
import { useVideo } from "../contexts/VideoContext";
import { useRecording } from "../contexts/RecordingContext";
import { useMarker } from "../contexts/MarkerContext";
import { usePreview } from "../hooks/usePreview";
import { Clapperboard, FolderOpen, Maximize, Pause, Play, Volume2, VolumeX } from "lucide-react";

const PLAYBACK_RATES = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2];

export function VideoPlayer() {
  const {
    videoRef,
    currentTime,
    duration,
    isPlaying,
    volume,
    playbackRate,
    videoSrc,
    togglePlay,
    setVolume,
    setPlaybackRate,
    loadVideo,
    toggleFullscreen,
    seek,
    updateTime,
    updateDuration,
    syncIsPlaying,
  } = useVideo();
  const { events } = useMarker();

  const {
    isPreviewing,
    isRecording,
    isInitializing,
    previewFrameUrl,
    captureWidth,
    captureHeight,
  } = useRecording();

  const canvasRef = usePreview({
    previewFrameUrl,
    width: captureWidth,
    height: captureHeight,
    enabled: isPreviewing || isRecording,
  });

  const fileInputRef = useRef<HTMLInputElement>(null);
  const progressRef = useRef<HTMLDivElement>(null);
  const [showSpeedMenu, setShowSpeedMenu] = useState(false);
  const [volumeBeforeMute, setVolumeBeforeMute] = useState(1);

  const showCanvas = isPreviewing || isRecording;
  const showVideo = !showCanvas && videoSrc;

  const formatTime = (seconds: number) => {
    if (!seconds || isNaN(seconds)) return "0:00";
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const handleVolumeToggle = () => {
    if (volume === 0) {
      setVolume(volumeBeforeMute > 0 ? volumeBeforeMute : 1);
    } else {
      setVolumeBeforeMute(volume);
      setVolume(0);
    }
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const url = URL.createObjectURL(file);
      loadVideo(url);
    }
  };

  const progress = duration > 0 ? (currentTime / duration) * 100 : 0;

  const handleProgressClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!progressRef.current || duration === 0) return;
    const rect = progressRef.current.getBoundingClientRect();
    const clickPosition = (e.clientX - rect.left) / rect.width;
    seek(clickPosition * duration);
  };

  return (
    <div className="h-full w-full">
      <div className="relative h-full w-full overflow-hidden rounded-[var(--radius-md)] border border-emerald-300/10 bg-neutral-950/90">
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-b from-emerald-500/5 via-transparent to-transparent" />
        {showCanvas && (
          <canvas
            ref={canvasRef}
            className="h-full w-full"
            style={{ objectFit: "contain" }}
          />
        )}

        {showVideo && (
          <video
            ref={videoRef}
            src={videoSrc || undefined}
            className="h-full w-full object-contain"
            preload="metadata"
            onLoadStart={() => {
              console.log("[VideoPlayer] Video load started", { src: videoSrc });
            }}
            onCanPlay={() => {
              console.log("[VideoPlayer] Video can play");
            }}
            onError={(event) => {
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
            onLoadedMetadata={(e) => updateDuration(e.currentTarget.duration)}
            onPlay={() => syncIsPlaying(true)}
            onPause={() => syncIsPlaying(false)}
            onEnded={() => syncIsPlaying(false)}
          />
        )}

        {!videoSrc && !showCanvas && (
          <div className="absolute inset-0 flex flex-col items-center justify-center">
            {isInitializing ? (
              <div className="flex flex-col items-center gap-3">
                <div className="w-8 h-8 border-2 border-emerald-400 border-t-transparent rounded-full animate-spin" />
                <p className="text-neutral-400 text-sm">Starting preview...</p>
              </div>
            ) : (
              <>
                <div className="mb-3 rounded-full border border-emerald-300/20 bg-emerald-500/10 p-2">
                  <Clapperboard className="h-5 w-5 text-emerald-200" />
                </div>
                <p className="text-neutral-400 mb-4">No recording loaded</p>
                <button
                  onClick={() => fileInputRef.current?.click()}
                  className="flex items-center gap-2 px-4 py-2 bg-white/5 hover:bg-white/10 rounded-md text-neutral-200 transition-colors border border-emerald-300/20"
                >
                  <FolderOpen className="w-4 h-4" />
                  Open File
                </button>
              </>
            )}
          </div>
        )}

        {showVideo && (
          <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-neutral-950/95 via-neutral-950/70 to-transparent p-4">
            <div className="flex items-center gap-3">
              <button
                onClick={togglePlay}
                className="text-white hover:text-emerald-100 transition-colors"
              >
                {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
              </button>

              <div className="flex items-center gap-3">
                <button
                  onClick={handleVolumeToggle}
                  className="text-white hover:text-emerald-100 transition-colors"
                >
                  {volume === 0 ? <VolumeX className="w-5 h-5" /> : <Volume2 className="w-5 h-5" />}
                </button>

                <div className="flex items-center gap-2">
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.05"
                    value={volume}
                    onChange={(e) => setVolume(parseFloat(e.target.value))}
                    className="w-20 h-3 appearance-none cursor-pointer bg-transparent
                            [&::-webkit-slider-thumb]:appearance-none 
                            [&::-webkit-slider-thumb]:w-3 
                            [&::-webkit-slider-thumb]:h-3 
                            [&::-webkit-slider-thumb]:rounded-full 
                            [&::-webkit-slider-thumb]:bg-white
                            [&::-webkit-slider-thumb]:cursor-pointer
                            [&::-webkit-slider-thumb]:mt-[-4px]
                            [&::-webkit-slider-runnable-track]:h-1
                            [&::-webkit-slider-runnable-track]:bg-neutral-600
                            [&::-webkit-slider-runnable-track]:rounded-full"
                  />
                  <span className="text-xs text-neutral-300 font-mono w-8 text-right">
                    {Math.round(volume * 100)}%
                  </span>
                </div>
              </div>

              <span className="text-xs text-white font-mono">
                {formatTime(currentTime)} / {formatTime(duration)}
              </span>

              <div
                ref={progressRef}
                className="group relative h-2 min-w-0 flex-1 cursor-pointer rounded-full bg-neutral-700/80 border border-emerald-300/10"
                onClick={handleProgressClick}
              >
                <div
                  className="h-full rounded-full bg-emerald-400/85 transition-colors"
                  style={{ width: `${progress}%` }}
                />
                <div
                  className="pointer-events-none absolute top-1/2 h-3 w-3 -translate-y-1/2 rounded-full bg-emerald-100 opacity-0 transition-opacity group-hover:opacity-100"
                  style={{ left: `calc(${progress}% - 6px)` }}
                />
                {events.map((event) => {
                  const position = duration > 0 ? (event.timestamp / duration) * 100 : 0;
                  const isDeath = event.type === "death";
                  const isManual = event.type === "manual";
                  const markerClassName = isManual
                    ? "h-2.5 w-2.5 rounded-sm bg-cyan-300"
                    : isDeath
                      ? "h-2.5 w-2.5 rounded-full bg-rose-300"
                      : "h-2.5 w-2.5 rounded-full bg-emerald-200";
                  return (
                    <button
                      key={event.id}
                      type="button"
                      className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 text-neutral-200 hover:text-white"
                      style={{ left: `${position}%` }}
                      onClick={(eventClick) => {
                        eventClick.stopPropagation();
                        seek(event.timestamp);
                      }}
                    >
                      <span className={`block ${markerClassName}`} />
                    </button>
                  );
                })}
              </div>

              <div className="relative">
                <button
                  onClick={() => setShowSpeedMenu(!showSpeedMenu)}
                  className="text-xs text-neutral-100 hover:text-emerald-200 px-2 py-1 bg-neutral-800 rounded border border-neutral-700 transition-colors"
                >
                  {playbackRate}x
                </button>
                {showSpeedMenu && (
                  <div className="absolute bottom-full mb-2 left-0 bg-neutral-900 rounded shadow-lg py-1 border border-neutral-700">
                    {PLAYBACK_RATES.map((rate) => (
                      <button
                        key={rate}
                        onClick={() => {
                          setPlaybackRate(rate);
                          setShowSpeedMenu(false);
                        }}
                        className={`block w-full text-left px-3 py-1 text-xs ${
                          playbackRate === rate
                            ? "text-emerald-300 bg-emerald-500/20"
                            : "text-neutral-300 hover:bg-neutral-800"
                        }`}
                      >
                        {rate}x
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <button
                onClick={toggleFullscreen}
                className="text-white hover:text-neutral-300 transition-colors"
              >
                <Maximize className="w-5 h-5" />
              </button>

              <button
                onClick={() => fileInputRef.current?.click()}
                className="text-white hover:text-neutral-300 transition-colors"
                title="Open Video"
              >
                <FolderOpen className="w-5 h-5" />
              </button>
            </div>
          </div>
        )}
      </div>

      <input
        ref={fileInputRef}
        type="file"
        accept="video/*"
        onChange={handleFileChange}
        className="hidden"
      />
    </div>
  );
}
