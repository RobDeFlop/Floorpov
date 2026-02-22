use tauri::{AppHandle, Emitter};

pub(super) fn emit_recording_stopped(app_handle: &AppHandle) {
    if let Err(error) = app_handle.emit("recording-stopped", ()) {
        tracing::error!("Failed to emit recording-stopped event: {error}");
    }
}

pub(super) fn emit_recording_finalized(app_handle: &AppHandle, output_path: &str) {
    if let Err(error) = app_handle.emit("recording-finalized", output_path) {
        tracing::error!("Failed to emit recording-finalized event: {error}");
    }
}

pub(super) fn emit_recording_warning(app_handle: &AppHandle, warning_message: &str) {
    if let Err(error) = app_handle.emit("recording-warning", warning_message.to_string()) {
        tracing::error!("Failed to emit recording-warning event: {error}");
    }
}

pub(super) fn emit_recording_warning_cleared(app_handle: &AppHandle) {
    if let Err(error) = app_handle.emit("recording-warning-cleared", ()) {
        tracing::error!("Failed to emit recording-warning-cleared event: {error}");
    }
}
