import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { check } from "@tauri-apps/plugin-updater";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { TitleBar } from "./TitleBar";
import { Sidebar } from "./Sidebar";
import { GameModePage } from "../gamemodes/GameModePage";
import { HomePage } from "./HomePage";
import { Settings, type SettingsGroupId } from "../settings/Settings";
import { CombatLogDebug } from "../debug/CombatLogDebug";
import { WarcraftLogsUploadPage } from "../warcraftlogs/WarcraftLogsUploadPage";
import { useVideo, VideoProvider } from "../../contexts/VideoContext";
import { RecordingProvider } from "../../contexts/RecordingContext";
import { SettingsProvider, useSettings } from "../../contexts/SettingsContext";
import { MarkerProvider } from "../../contexts/MarkerContext";
import { WclUploadProvider } from "../../contexts/WclUploadContext";
import { panelVariants, smoothTransition } from "../../lib/motion";
import { type AppView } from "../../types/ui";

export type { AppView };
const GAME_MODE_VIEWS = new Set<AppView>(["mythic-plus", "raid", "pvp"]);
const AUTO_UPDATE_SESSION_FLAG = "floorpov:auto-update-check-ran";

function LayoutContent() {
  const { settings, isLoading: isSettingsLoading } = useSettings();
  const { isFullscreen } = useVideo();
  const hasAttemptedAutoUpdateRef = useRef(false);
  const autoUpdateDownloadedBytesRef = useRef(0);
  const autoUpdateContentLengthRef = useRef<number | null>(null);
  const [currentView, setCurrentView] = useState<AppView>("main");
  const [settingsHasChanges, setSettingsHasChanges] = useState(false);
  const [openSettingsGroups, setOpenSettingsGroups] = useState<Record<SettingsGroupId, boolean>>({
    recording: true,
    storage: false,
    wow: false,
    controls: false,
    app: false,
  });
  const [pendingSettingsNavigation, setPendingSettingsNavigation] = useState<AppView | null>(null);
  const [gameModeNavigationVersion, setGameModeNavigationVersion] = useState(0);
  const [isDebugBuild, setIsDebugBuild] = useState(false);
  const [autoUpdateBannerText, setAutoUpdateBannerText] = useState<string | null>(null);
  const reduceMotion = useReducedMotion();

  useEffect(() => {
    const loadDebugFlag = async () => {
      try {
        const debugEnabled = await invoke<boolean>("is_debug_build");
        setIsDebugBuild(debugEnabled);
      } catch (error) {
        console.error("Failed to load debug build flag:", error);
        setIsDebugBuild(false);
      }
    };

    loadDebugFlag();
  }, []);

  useEffect(() => {
    if (!isDebugBuild && currentView === "debug") {
      setCurrentView("main");
    }
  }, [currentView, isDebugBuild]);

  useEffect(() => {
    if (!isTauri() || isSettingsLoading || !settings.enableAutoUpdate || hasAttemptedAutoUpdateRef.current) {
      return;
    }

    hasAttemptedAutoUpdateRef.current = true;
    let isCancelled = false;

    const runAutoUpdate = async () => {
      try {
        if (typeof window !== "undefined") {
          if (window.sessionStorage.getItem(AUTO_UPDATE_SESSION_FLAG) === "1") {
            return;
          }

          window.sessionStorage.setItem(AUTO_UPDATE_SESSION_FLAG, "1");
        }

        const update = await check();
        if (!update || isCancelled) {
          return;
        }

        setAutoUpdateBannerText("Update found. Downloading and installing...");
        autoUpdateDownloadedBytesRef.current = 0;
        autoUpdateContentLengthRef.current = null;

        await update.downloadAndInstall((event) => {
          if (isCancelled) {
            return;
          }

          switch (event.event) {
            case "Started": {
              const contentLength = event.data.contentLength;
              autoUpdateContentLengthRef.current = contentLength ?? null;
              if (!contentLength || contentLength <= 0) {
                setAutoUpdateBannerText("Update found. Downloading and installing...");
                return;
              }

              setAutoUpdateBannerText("Update found. Downloading update (0%)...");
              return;
            }
            case "Progress": {
              autoUpdateDownloadedBytesRef.current += event.data.chunkLength;
              const contentLength = autoUpdateContentLengthRef.current;

              if (contentLength && contentLength > 0) {
                const progressPercent = Math.min(
                  99,
                  Math.floor((autoUpdateDownloadedBytesRef.current / contentLength) * 100),
                );
                setAutoUpdateBannerText(`Update found. Downloading update (${progressPercent}%)...`);
                return;
              }

              const downloadedMiB = autoUpdateDownloadedBytesRef.current / (1024 * 1024);
              setAutoUpdateBannerText(
                `Update found. Downloaded ${downloadedMiB.toFixed(1)} MiB...`,
              );
              return;
            }
            case "Finished": {
              autoUpdateContentLengthRef.current = null;
              setAutoUpdateBannerText("Download complete. Installing update...");
            }
          }
        });

        setAutoUpdateBannerText("Update installed. Restarting app...");
      } catch (error) {
        if (!isCancelled) {
          console.error("Auto-update check failed:", error);
          setAutoUpdateBannerText(null);
        }
      }
    };

    void runAutoUpdate();

    return () => {
      isCancelled = true;
    };
  }, [isSettingsLoading, settings.enableAutoUpdate]);

  const navigateWithoutGuard = (view: AppView) => {
    setPendingSettingsNavigation(null);
    setCurrentView(view);

    if (GAME_MODE_VIEWS.has(view)) {
      setGameModeNavigationVersion((currentVersion) => currentVersion + 1);
    }
  };

  const handleNavigate = (view: AppView) => {
    if (view === currentView) {
      return;
    }

    if (currentView === "settings" && settingsHasChanges) {
      setPendingSettingsNavigation(view);
      return;
    }

    navigateWithoutGuard(view);
  };

  const handleSettingsNavigationHandled = () => {
    setPendingSettingsNavigation(null);
  };

  const handleSettingsGroupToggle = (groupId: SettingsGroupId, open: boolean) => {
    setOpenSettingsGroups((currentGroups) => ({
      ...currentGroups,
      [groupId]: open,
    }));
  };

  return (
    <div className="relative h-screen w-screen flex flex-col bg-neutral-950 text-neutral-100 overflow-hidden">
      {autoUpdateBannerText && (
        <div
          className="pointer-events-none absolute right-4 top-14 z-50 rounded-sm border border-amber-300/30 bg-amber-500/12 px-3 py-2 text-xs text-amber-100 shadow-(--surface-glow)"
          role="status"
          aria-live="polite"
        >
          {autoUpdateBannerText}
        </div>
      )}
      {!isFullscreen && <TitleBar />}
      <div
        className={
          isFullscreen
            ? "flex min-h-0 flex-1 overflow-hidden"
            : "flex flex-1 min-h-0 flex-col gap-2 overflow-hidden p-2 md:flex-row md:gap-3 md:p-3"
        }
      >
        {!isFullscreen && (
          <Sidebar
            onNavigate={handleNavigate}
            currentView={currentView}
            isDebugMode={isDebugBuild}
          />
        )}
        <AnimatePresence mode="wait" initial={false}>
          {currentView === "main" ? (
            <motion.div
              key="main-view"
              className="flex-1 flex flex-col min-w-0 rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <HomePage />
            </motion.div>
          ) : currentView === "settings" ? (
            <motion.div
              key="settings-view"
              className="h-full flex-1 min-w-0 min-h-0 flex flex-col rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <Settings
                navigationRequest={pendingSettingsNavigation}
                openGroups={openSettingsGroups}
                onDirtyChange={setSettingsHasChanges}
                onGroupToggle={handleSettingsGroupToggle}
                onNavigationHandled={handleSettingsNavigationHandled}
                onNavigateWithoutGuard={navigateWithoutGuard}
              />
            </motion.div>
          ) : currentView === "warcraftlogs" ? (
            <motion.div
              key="warcraftlogs-view"
              className="h-full flex-1 min-w-0 min-h-0 flex flex-col rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <WarcraftLogsUploadPage />
            </motion.div>
          ) : currentView === "mythic-plus" ? (
            <motion.div
              key="mythic-plus-view"
              className="h-full flex-1 min-w-0 min-h-0 flex flex-col rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <GameModePage
                key={`mythic-plus-page-${gameModeNavigationVersion}`}
                gameMode="mythic-plus"
              />
            </motion.div>
          ) : currentView === "raid" ? (
            <motion.div
              key="raid-view"
              className="h-full flex-1 min-w-0 min-h-0 flex flex-col rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <GameModePage key={`raid-page-${gameModeNavigationVersion}`} gameMode="raid" />
            </motion.div>
          ) : currentView === "pvp" ? (
            <motion.div
              key="pvp-view"
              className="h-full flex-1 min-w-0 min-h-0 flex flex-col rounded-sm border border-white/10 bg-(--surface-1) shadow-(--surface-glow) overflow-hidden"
              variants={panelVariants}
              initial={reduceMotion ? false : "initial"}
              animate="animate"
              exit={reduceMotion ? undefined : "exit"}
              transition={smoothTransition}
            >
              <GameModePage key={`pvp-page-${gameModeNavigationVersion}`} gameMode="pvp" />
            </motion.div>
          ) : (
            <CombatLogDebug />
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

export function Layout() {
  return (
    <VideoProvider>
      <SettingsProvider>
        <MarkerProvider>
          <RecordingProvider>
            <WclUploadProvider>
              <LayoutContent />
            </WclUploadProvider>
          </RecordingProvider>
        </MarkerProvider>
      </SettingsProvider>
    </VideoProvider>
  );
}
