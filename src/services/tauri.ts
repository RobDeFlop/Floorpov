// src/services/tauri.ts
import { invoke } from '@tauri-apps/api/core';
import { RecordingStartedPayload } from '../types/recording';
import { RecordingSettings } from '../types/settings';

export interface TauriError {
  message: string;
  code?: number;
}

export type RecordingErrorType = 
  | 'storage'
  | 'ffmpeg'
  | 'audio_capture'
  | 'video_capture';

export interface RecordingError {
  type: RecordingErrorType;
  message: string;
}

export function getErrorMessage(error: unknown): string {
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

  return String(error);
}

const ERROR_PREFIX_MAP: Record<string, RecordingErrorType> = {
  "Storage error:": "storage",
  "FFmpeg error:": "ffmpeg",
  "Audio capture error:": "audio_capture",
  "Video capture error:": "video_capture",
};

function detectErrorType(errorMessage: string): RecordingErrorType {
  for (const [prefix, type] of Object.entries(ERROR_PREFIX_MAP)) {
    if (errorMessage.startsWith(prefix)) {
      return type;
    }
  }
  return "storage";
}

function handleTauriError(error: unknown): RecordingError {
  const errorMessage = getErrorMessage(error);
  const errorType = detectErrorType(errorMessage);

  return { type: errorType, message: errorMessage };
}

// Typed API methods
export async function startRecording(
  settings: RecordingSettings,
  outputFolder: string,
  maxStorageBytes: number
): Promise<RecordingStartedPayload> {
  try {
    const result = await invoke<RecordingStartedPayload>('start_recording', {
      settings,
      outputFolder,
      maxStorageBytes
    });
    return result;
  } catch (error) {
    throw handleTauriError(error);
  }
}

export async function stopRecording(): Promise<string> {
  try {
    const result = await invoke<string>('stop_recording');
    return result;
  } catch (error) {
    throw handleTauriError(error);
  }
}

export async function listCaptureWindows(): Promise<unknown[]> {
  try {
    const result = await invoke<unknown[]>('list_capture_windows');
    return result;
  } catch (error) {
    throw handleTauriError(error);
  }
}

// Type guard to check if an error is a RecordingError
export function isRecordingError(error: unknown): error is RecordingError {
  return typeof error === 'object' && error !== null && 
         'type' in error && 'message' in error;
}