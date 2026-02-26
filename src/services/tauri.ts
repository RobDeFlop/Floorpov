// src/services/tauri.ts
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
