// src/types/recording.ts
export interface RecordingStartedPayload {
  output_path: string;
  width: number;
  height: number;
}

export interface CaptureWindowInfo {
  hwnd: string;
  title: string;
  process_name: string | null;
}

export interface CleanupResult {
  deleted_count: number;
  freed_bytes: number;
  deleted_files: string[];
}
