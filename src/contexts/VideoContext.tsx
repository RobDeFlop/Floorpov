import { createContext, useContext, useState, useRef, useCallback, useEffect, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { VIDEO_LOADING_TIMEOUT_MS, VOLUME_MAX, VOLUME_MIN } from "../types/settings";

interface VideoContextType {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  currentTime: number;
  duration: number;
  isPlaying: boolean;
  isVideoLoading: boolean;
  volume: number;
  playbackRate: number;
  videoSrc: string | null;
  togglePlay: () => void;
  seek: (time: number) => void;
  setVolume: (volume: number) => void;
  setPlaybackRate: (rate: number) => void;
  loadVideo: (src: string) => void;
  updateTime: (time: number) => void;
  updateDuration: (duration: number) => void;
  syncIsPlaying: (playing: boolean) => void;
  setVideoLoading: (loading: boolean) => void;
  isFullscreen: boolean;
  fullscreenPhase: FullscreenPhase;
  toggleFullscreen: () => Promise<void>;
  exitFullscreen: () => Promise<void>;
}

const VideoContext = createContext<VideoContextType | null>(null);
type FullscreenPhase = "windowed" | "entering" | "fullscreen" | "exiting";

export function VideoProvider({ children }: { children: ReactNode }) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const objectUrlRef = useRef<string | null>(null);
  const loadingTimeoutRef = useRef<number | null>(null);
  const videoSrcRef = useRef<string | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isVideoLoading, setIsVideoLoading] = useState(false);
  const [volume, setVolumeState] = useState(1);
  const [playbackRate, setPlaybackRateState] = useState(1);
  const [videoSrc, setVideoSrc] = useState<string | null>(null);
  const [fullscreenPhase, setFullscreenPhaseState] = useState<FullscreenPhase>("windowed");
  const fullscreenPhaseRef = useRef<FullscreenPhase>("windowed");
  const fullscreenTransitionRef = useRef<Promise<void> | null>(null);
  const closeAfterFullscreenExitRef = useRef(false);
  const isFullscreen = fullscreenPhase !== "windowed";

  const togglePlay = useCallback(() => {
    if (!videoRef.current) return;
    if (videoRef.current.paused) {
      videoRef.current.play();
    } else {
      videoRef.current.pause();
    }
  }, []);

  const seek = useCallback((time: number) => {
    if (videoRef.current) {
      videoRef.current.currentTime = time;
    }
  }, []);

  const updateTime = useCallback((time: number) => {
    setCurrentTime(time);
  }, []);

  const updateDuration = useCallback((dur: number) => {
    setDuration(dur);
  }, []);

  const syncIsPlaying = useCallback((playing: boolean) => {
    setIsPlaying(playing);
  }, []);

  const setVideoLoading = useCallback((loading: boolean) => {
    if (loadingTimeoutRef.current !== null) {
      clearTimeout(loadingTimeoutRef.current);
      loadingTimeoutRef.current = null;
    }

    setIsVideoLoading(loading);

    if (loading) {
      loadingTimeoutRef.current = window.setTimeout(() => {
        setIsVideoLoading(false);
        loadingTimeoutRef.current = null;
      }, VIDEO_LOADING_TIMEOUT_MS);
    }
  }, []);

  const setVolume = useCallback((vol: number) => {
    const nextVolume = Math.min(VOLUME_MAX, Math.max(VOLUME_MIN, vol));

    if (videoRef.current) {
      videoRef.current.volume = nextVolume;
    }

    setVolumeState(nextVolume);
  }, []);

  const setPlaybackRate = useCallback((rate: number) => {
    if (!Number.isFinite(rate) || rate <= 0) {
      return;
    }

    if (videoRef.current) {
      videoRef.current.playbackRate = rate;
    }
    setPlaybackRateState(rate);
  }, []);

  const setFullscreenPhase = useCallback((phase: FullscreenPhase) => {
    fullscreenPhaseRef.current = phase;
    setFullscreenPhaseState(phase);
  }, []);

  const exitFullscreen = useCallback(async () => {
    if (!isTauri()) {
      return;
    }

    try {
      await fullscreenTransitionRef.current;
    } catch {
      return;
    }

    if (fullscreenPhaseRef.current !== "fullscreen") {
      return;
    }

    setFullscreenPhase("exiting");
    const transition = invoke<void>("exit_playback_fullscreen");
    fullscreenTransitionRef.current = transition;

    try {
      await transition;
      setFullscreenPhase("windowed");
    } catch (error) {
      setFullscreenPhase("fullscreen");
      console.error("Fullscreen exit failed:", error);
    } finally {
      if (fullscreenTransitionRef.current === transition) {
        fullscreenTransitionRef.current = null;
      }
    }
  }, [setFullscreenPhase]);

  const toggleFullscreen = useCallback(async () => {
    if (!isTauri()) {
      return;
    }

    if (fullscreenPhaseRef.current === "fullscreen") {
      await exitFullscreen();
      return;
    }

    if (fullscreenPhaseRef.current !== "windowed") {
      return;
    }

    setFullscreenPhase("entering");
    const transition = invoke<void>("enter_playback_fullscreen");
    fullscreenTransitionRef.current = transition;

    try {
      await transition;
      setFullscreenPhase("fullscreen");
    } catch (error) {
      console.error("Fullscreen entry failed:", error);
      try {
        await invoke<void>("exit_playback_fullscreen");
        setFullscreenPhase("windowed");
      } catch (rollbackError) {
        setFullscreenPhase("fullscreen");
        console.error("Fullscreen entry rollback failed:", rollbackError);
      }
    } finally {
      if (fullscreenTransitionRef.current === transition) {
        fullscreenTransitionRef.current = null;
      }
    }
  }, [exitFullscreen, setFullscreenPhase]);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    const appWindow = getCurrentWindow();
    let isCancelled = false;
    let unsubscribe: (() => void) | undefined;

    void appWindow
      .onCloseRequested((event) => {
        if (
          fullscreenPhaseRef.current === "windowed" ||
          closeAfterFullscreenExitRef.current
        ) {
          return;
        }

        event.preventDefault();
        closeAfterFullscreenExitRef.current = true;

        void (async () => {
          try {
            await exitFullscreen();

            if (fullscreenPhaseRef.current === "windowed") {
              await appWindow.close();
              return;
            }
          } finally {
            closeAfterFullscreenExitRef.current = false;
          }
        })();
      })
      .then((unlisten) => {
        if (isCancelled) {
          unlisten();
        } else {
          unsubscribe = unlisten;
        }
      })
      .catch((error) => {
        console.error("Fullscreen close listener failed:", error);
      });

    return () => {
      isCancelled = true;
      unsubscribe?.();
    };
  }, [exitFullscreen]);

  const loadVideo = useCallback(
    (src: string) => {
      const currentSrc = videoSrcRef.current;
      if (src === currentSrc) {
        if (videoRef.current) {
          videoRef.current.pause();
          videoRef.current.currentTime = 0;
        }
        setCurrentTime(0);
        setIsPlaying(false);
        setVideoLoading(false);
        return;
      }

      if (objectUrlRef.current && objectUrlRef.current !== src) {
        URL.revokeObjectURL(objectUrlRef.current);
        objectUrlRef.current = null;
      }

      if (src.startsWith("blob:")) {
        objectUrlRef.current = src;
      }

      videoSrcRef.current = src;
      setVideoSrc(src);
      setCurrentTime(0);
      setDuration(0);
      setIsPlaying(false);
      setVideoLoading(true);
    },
    [setVideoLoading]
  );

  useEffect(() => {
    return () => {
      if (loadingTimeoutRef.current !== null) {
        clearTimeout(loadingTimeoutRef.current);
      }
      if (objectUrlRef.current) {
        URL.revokeObjectURL(objectUrlRef.current);
      }
    };
  }, []);

  return (
    <VideoContext.Provider
      value={{
        videoRef,
        currentTime,
        duration,
        isPlaying,
        isVideoLoading,
        volume,
        playbackRate,
        videoSrc,
        togglePlay,
        seek,
        setVolume,
        setPlaybackRate,
        loadVideo,
        updateTime,
        updateDuration,
        syncIsPlaying,
        setVideoLoading,
        isFullscreen,
        fullscreenPhase,
        toggleFullscreen,
        exitFullscreen,
      }}
    >
      {children}
    </VideoContext.Provider>
  );
}

export function useVideo() {
  const context = useContext(VideoContext);
  if (!context) {
    throw new Error("useVideo must be used within a VideoProvider");
  }
  return context;
}
