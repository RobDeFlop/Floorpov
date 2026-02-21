import { useState, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import {
  ArrowLeft,
  CheckCircle2,
  Folder,
  HardDrive,
  Keyboard,
  Mic,
  Monitor,
  RotateCw,
  Settings2,
  Video,
  Volume2,
  XCircle,
} from 'lucide-react';
import { useSettings } from '../contexts/SettingsContext';
import { useRecording } from '../contexts/RecordingContext';
import { RecordingSettings, QUALITY_SETTINGS, MIN_STORAGE_GB, MAX_STORAGE_GB, HOTKEY_OPTIONS } from '../types/settings';

interface SettingsProps {
  onBack: () => void;
}

interface WindowOption {
  id: string;
  title: string;
  processName?: string;
}

export function Settings({ onBack }: SettingsProps) {
  const { settings, updateSettings } = useSettings();
  const { isRecording } = useRecording();
  const [formData, setFormData] = useState<RecordingSettings>(settings);
  const [folderSize, setFolderSize] = useState<number>(0);
  const [isWowFolderValid, setIsWowFolderValid] = useState<boolean>(false);
  const [hasChanges, setHasChanges] = useState(false);
  const [availableWindows, setAvailableWindows] = useState<WindowOption[]>([]);
  const [isLoadingWindows, setIsLoadingWindows] = useState(false);
  const [windowsError, setWindowsError] = useState<string | null>(null);

  useEffect(() => {
    setFormData(settings);
  }, [settings]);

  useEffect(() => {
    if (formData.outputFolder) {
      loadFolderSize();
    }
  }, [formData.outputFolder]);

  useEffect(() => {
    let isMounted = true;

    const validateWowFolder = async () => {
      if (!formData.wowFolder) {
        if (isMounted) {
          setIsWowFolderValid(false);
        }
        return;
      }

      try {
        const isValid = await invoke<boolean>('validate_wow_folder', {
          path: formData.wowFolder,
        });

        if (isMounted) {
          setIsWowFolderValid(isValid);
        }
      } catch (error) {
        if (isMounted) {
          setIsWowFolderValid(false);
        }
        console.error('Failed to validate WoW folder:', error);
      }
    };

    validateWowFolder();

    return () => {
      isMounted = false;
    };
  }, [formData.wowFolder]);

  useEffect(() => {
    setHasChanges(JSON.stringify(formData) !== JSON.stringify(settings));
  }, [formData, settings]);

  useEffect(() => {
    if (formData.captureSource === "window") {
      void loadWindows();
    } else {
      setWindowsError(null);
    }
  }, [formData.captureSource]);

  useEffect(() => {
    if (formData.captureSource !== "window") {
      return;
    }

    if (availableWindows.length === 0) {
      return;
    }

    if (!formData.selectedWindow) {
      setFormData((previous) => ({ ...previous, selectedWindow: availableWindows[0].id }));
      return;
    }

    const hasMatchingId = availableWindows.some((windowOption) => {
      return windowOption.id === formData.selectedWindow;
    });

    if (hasMatchingId) {
      return;
    }

    const matchByTitle = availableWindows.find((windowOption) => {
      return windowOption.title === formData.selectedWindow;
    });

    if (matchByTitle) {
      setFormData((previous) => ({ ...previous, selectedWindow: matchByTitle.id }));
      return;
    }

    setFormData((previous) => ({ ...previous, selectedWindow: availableWindows[0].id }));
  }, [availableWindows, formData.captureSource, formData.selectedWindow]);

  const getErrorMessage = (error: unknown): string => {
    if (typeof error === "string") {
      return error;
    }

    if (error && typeof error === "object") {
      const maybeMessage = (error as { message?: unknown }).message;
      if (typeof maybeMessage === "string") {
        return maybeMessage;
      }

      const maybeError = (error as { error?: unknown }).error;
      if (typeof maybeError === "string") {
        return maybeError;
      }
    }

    return "Failed to load available windows.";
  };

  const loadWindows = async () => {
    setIsLoadingWindows(true);
    setWindowsError(null);
    try {
      const windows = await invoke<WindowOption[]>("list_windows");
      setAvailableWindows(windows);
      if (windows.length === 0) {
        setWindowsError("No capturable windows found. Open a window and refresh.");
      }
    } catch (error) {
      setAvailableWindows([]);
      setWindowsError(getErrorMessage(error));
      console.error("Failed to list windows:", error);
    } finally {
      setIsLoadingWindows(false);
    }
  };

  const loadFolderSize = async () => {
    try {
      const size = await invoke<number>('get_folder_size', { 
        path: formData.outputFolder 
      });
      setFolderSize(size);
    } catch (error) {
      console.error('Failed to get folder size:', error);
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: formData.outputFolder,
      });
      
      if (selected && typeof selected === 'string') {
        setFormData({ ...formData, outputFolder: selected });
      }
    } catch (error) {
      console.error('Failed to open folder picker:', error);
    }
  };

  const handleSave = async () => {
    if (formData.maxStorageGB < MIN_STORAGE_GB) {
      return;
    }
    
    if (formData.maxStorageGB > MAX_STORAGE_GB) {
      return;
    }

    if (formData.captureSource === "window" && !formData.selectedWindow) {
      return;
    }

    try {
      await updateSettings(formData);
      setHasChanges(false);
    } catch (error) {
      // Error already logged in context
    }
  };

  const handleCancel = () => {
    setFormData(settings);
    setHasChanges(false);
  };

  const handleBrowseWowFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: formData.wowFolder || formData.outputFolder,
      });

      if (selected && typeof selected === 'string') {
        setFormData({ ...formData, wowFolder: selected });
      }
    } catch (error) {
      console.error('Failed to open WoW folder picker:', error);
    }
  };

  const formatBytes = (bytes: number) => {
    const gb = bytes / (1024 ** 3);
    return gb.toFixed(2) + ' GB';
  };

  const usagePercentage = formData.maxStorageGB > 0 
    ? Math.min(100, (folderSize / (formData.maxStorageGB * 1024 ** 3)) * 100)
    : 0;

  const fieldClassName =
    'w-full rounded-md border border-emerald-300/20 bg-black/20 px-3 py-2 text-sm text-neutral-100 transition-colors placeholder:text-neutral-500 focus:border-emerald-300/35';
  const readOnlyFieldClassName =
    'flex-1 rounded-md border border-emerald-300/20 bg-black/20 px-3 py-2 text-sm text-neutral-300';
  const sectionClassName =
    'rounded-[var(--radius-md)] border border-emerald-300/10 bg-[var(--surface-1)]/80 p-4';
  const sectionHeadingClassName =
    'mb-4 inline-flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.13em] text-emerald-200';
  const browseButtonClassName =
    'inline-flex items-center gap-2 rounded-md border border-emerald-300/25 bg-emerald-500/12 px-4 py-2 text-sm text-emerald-100 transition-colors hover:bg-emerald-500/20';

  return (
    <div className="relative flex flex-1 min-h-0 flex-col overflow-hidden bg-[var(--surface-0)]">
      {isRecording && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm">
          <div className="max-w-md rounded-[var(--radius-md)] border border-rose-300/25 bg-[var(--surface-2)] p-8 text-center shadow-[var(--surface-glow)]">
            <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-rose-500/20">
              <div className="h-3 w-3 rounded-full bg-rose-400 animate-pulse" />
            </div>
            <h2 className="mb-2 text-xl font-semibold text-rose-100">Recording in Progress</h2>
            <p className="text-neutral-300">
              Stop recording to change settings
            </p>
          </div>
        </div>
      )}

      <div className="shrink-0 border-b border-emerald-300/10 bg-[var(--surface-1)] px-6 py-4 flex items-center gap-4">
        <div className="flex items-center gap-3">
          <button
            onClick={onBack}
            className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-emerald-300/20 bg-black/20 text-neutral-200 transition-colors hover:bg-white/5"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
          <div>
            <h1 className="inline-flex items-center gap-2 text-lg font-semibold text-neutral-100">
              <Settings2 className="h-4 w-4 text-emerald-300" />
              Settings
            </h1>
            <p className="text-xs uppercase tracking-[0.12em] text-neutral-500">Capture and recording configuration</p>
          </div>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-6 py-6 pb-10">
        <div className="mx-auto w-full max-w-6xl space-y-4">
          <section className={sectionClassName}>
            <h2 className={sectionHeadingClassName}>
              <Video className="h-4 w-4" />
              Video
            </h2>
            <div className="grid gap-4 md:grid-cols-2">
              <div>
                <label className="mb-2 block text-sm text-neutral-300">Video Quality</label>
                <select
                  value={formData.videoQuality}
                  onChange={(e) => setFormData({ ...formData, videoQuality: e.target.value as any })}
                  className={fieldClassName}
                >
                  {Object.entries(QUALITY_SETTINGS).map(([key, { label }]) => (
                    <option key={key} value={key}>{label}</option>
                  ))}
                </select>
                <p className="mt-1 text-xs text-neutral-500">
                  Higher quality uses more disk space
                </p>
              </div>

              <div>
                <label className="mb-2 block text-sm text-neutral-300">Frame Rate</label>
                <select
                  value={formData.frameRate}
                  onChange={(e) => setFormData({ ...formData, frameRate: parseInt(e.target.value) as any })}
                  className={fieldClassName}
                >
                  <option value={30}>30 FPS</option>
                  <option value={60}>60 FPS</option>
                </select>
              </div>
            </div>
          </section>

          <section className={sectionClassName}>
            <h2 className={sectionHeadingClassName}>
              <HardDrive className="h-4 w-4" />
              Output
            </h2>
            <div className="space-y-4">
              <div>
                <label className="mb-2 block text-sm text-neutral-300">Output Folder</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={formData.outputFolder}
                    readOnly
                    className={readOnlyFieldClassName}
                  />
                  <button
                    onClick={handleBrowseFolder}
                    className={browseButtonClassName}
                  >
                    <Folder className="w-4 h-4" />
                    Browse
                  </button>
                </div>
                <div className="mt-3 rounded-md border border-emerald-300/10 bg-black/20 p-3">
                  <div className="mb-2 flex items-center justify-between text-xs text-neutral-400">
                    <span>Current usage</span>
                    <span className="font-mono text-neutral-200">
                      {formatBytes(folderSize)} / {formData.maxStorageGB} GB ({usagePercentage.toFixed(0)}%)
                    </span>
                  </div>
                  <div className="h-2 overflow-hidden rounded-full bg-neutral-800">
                    <div
                      className="h-full rounded-full bg-emerald-400/80"
                      style={{ width: `${usagePercentage}%` }}
                    />
                  </div>
                </div>
              </div>

              <div>
                <label className="mb-2 block text-sm text-neutral-300">
                  Maximum Storage (GB)
                </label>
                <input
                  type="number"
                  min={MIN_STORAGE_GB}
                  max={MAX_STORAGE_GB}
                  value={formData.maxStorageGB}
                  onChange={(e) => setFormData({ ...formData, maxStorageGB: parseInt(e.target.value) || MIN_STORAGE_GB })}
                  className={fieldClassName}
                />
                <p className="mt-1 text-xs text-neutral-500">
                  Old recordings will be automatically deleted when this limit is reached (minimum {MIN_STORAGE_GB} GB)
                </p>
              </div>
            </div>
          </section>

          <section className={sectionClassName}>
            <h2 className={sectionHeadingClassName}>
              <Monitor className="h-4 w-4" />
              Capture
            </h2>
            <div className="space-y-4">
              <div>
                <label className="mb-2 block text-sm text-neutral-300">Capture Source</label>
                <select
                  value={formData.captureSource}
                  onChange={(e) =>
                    setFormData({
                      ...formData,
                      captureSource: e.target.value as RecordingSettings["captureSource"],
                    })
                  }
                  className={fieldClassName}
                >
                  <option value="primary-monitor">Primary Monitor</option>
                  <option value="window">Specific Window</option>
                </select>
              </div>

              {formData.captureSource === "window" && (
                <div>
                  <div className="mb-2 flex items-center justify-between">
                    <label className="block text-sm text-neutral-300">Window</label>
                    <button
                      type="button"
                      onClick={loadWindows}
                      disabled={isLoadingWindows}
                      className="inline-flex items-center gap-1.5 rounded-md border border-emerald-300/20 bg-black/20 px-2.5 py-1 text-xs text-neutral-200 transition-colors hover:bg-white/5 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      <RotateCw className={`h-3.5 w-3.5 ${isLoadingWindows ? "animate-spin" : ""}`} />
                      Refresh
                    </button>
                  </div>
                  <select
                    value={formData.selectedWindow || ""}
                    onChange={(e) => setFormData({ ...formData, selectedWindow: e.target.value })}
                    className={fieldClassName}
                    disabled={availableWindows.length === 0 || isLoadingWindows}
                  >
                    <option value="" disabled>
                      {isLoadingWindows ? "Loading windows..." : "Select a window"}
                    </option>
                    {availableWindows.map((windowOption) => (
                      <option key={windowOption.id} value={windowOption.id}>
                        {windowOption.processName
                          ? `${windowOption.title} (${windowOption.processName})`
                          : windowOption.title}
                      </option>
                    ))}
                  </select>
                  {windowsError && (
                    <p className="mt-1 text-xs text-rose-300">{windowsError}</p>
                  )}
                  {!windowsError && availableWindows.length > 0 && (
                    <p className="mt-1 text-xs text-neutral-500">
                      Pick the app window to preview and record.
                    </p>
                  )}
                </div>
              )}
            </div>
          </section>

          <section className={sectionClassName}>
            <h2 className={sectionHeadingClassName}>
              <CheckCircle2 className="h-4 w-4" />
              Combat Log
            </h2>
            <div className="space-y-4">
              <div>
                <label className="mb-2 block text-sm text-neutral-300">WoW Folder</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={formData.wowFolder}
                    readOnly
                    className={readOnlyFieldClassName}
                  />
                  <button
                    onClick={handleBrowseWowFolder}
                    className={browseButtonClassName}
                  >
                    <Folder className="w-4 h-4" />
                    Browse
                  </button>
                </div>
                {!formData.wowFolder && (
                  <p className="mt-2 text-xs text-neutral-500">
                    Select your WoW installation folder. Floorpov reads combat events from Logs/WoWCombatLog.txt.
                  </p>
                )}
                {formData.wowFolder && isWowFolderValid && (
                  <p className="mt-2 inline-flex items-center gap-1.5 text-xs text-emerald-300">
                    <CheckCircle2 className="h-3.5 w-3.5" />
                    Combat log found at Logs/WoWCombatLog.txt.
                  </p>
                )}
                {formData.wowFolder && !isWowFolderValid && (
                  <p className="mt-2 inline-flex items-center gap-1.5 text-xs text-rose-300">
                    <XCircle className="h-3.5 w-3.5" />
                    Could not find Logs/WoWCombatLog.txt in this folder.
                  </p>
                )}
              </div>
            </div>
          </section>

          <section className={sectionClassName}>
            <h2 className={sectionHeadingClassName}>
              <Keyboard className="h-4 w-4" />
              Hotkeys
            </h2>
            <div className="space-y-4">
              <div>
                <label className="mb-2 block text-sm text-neutral-300">Manual Marker Hotkey</label>
                <select
                  value={formData.markerHotkey}
                  onChange={(e) => setFormData({ ...formData, markerHotkey: e.target.value as any })}
                  className={fieldClassName}
                >
                  {HOTKEY_OPTIONS.map(({ value, label }) => (
                    <option key={value} value={value}>{label}</option>
                  ))}
                </select>
                <p className="mt-1 text-xs text-neutral-500">
                  Press this key during recording to add a manual marker. If the key is already in use by another application, try a different one.
                </p>
              </div>
            </div>
          </section>

          <section className={`${sectionClassName} opacity-70`}>
            <h2 className={sectionHeadingClassName}>
              <Volume2 className="h-4 w-4" />
              Audio
            </h2>
            <div className="space-y-4">
              <p className="text-sm text-neutral-500">Audio recording will be available in a later phase.</p>

              <label className="flex items-center gap-3 cursor-not-allowed rounded-md border border-emerald-300/10 bg-black/20 px-3 py-2">
                <input
                  type="checkbox"
                  disabled
                  checked={formData.enableSystemAudio}
                  className="w-4 h-4"
                />
                <span className="text-sm">Enable System Audio</span>
              </label>

              <label className="flex items-center gap-3 cursor-not-allowed rounded-md border border-emerald-300/10 bg-black/20 px-3 py-2">
                <input
                  type="checkbox"
                  disabled
                  checked={formData.enableMicrophone}
                  className="w-4 h-4"
                />
                <span className="inline-flex items-center gap-2 text-sm">
                  <Mic className="h-3.5 w-3.5" />
                  Enable Microphone
                </span>
              </label>
            </div>
          </section>
        </div>
      </div>

      <div className="shrink-0 border-t border-emerald-300/10 bg-[var(--surface-1)] px-6 py-4 flex justify-end gap-3">
        <button
          onClick={handleCancel}
          disabled={!hasChanges}
          className="rounded-md border border-emerald-300/20 bg-black/20 px-4 py-2 text-sm font-medium text-neutral-200 transition-colors hover:bg-white/5 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Cancel
        </button>
        <button
          onClick={handleSave}
          disabled={!hasChanges}
          className="rounded-md border border-emerald-300/35 bg-emerald-500/20 px-4 py-2 text-sm font-semibold text-emerald-100 transition-colors hover:bg-emerald-500/30 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Save Changes
        </button>
      </div>
    </div>
  );
}
