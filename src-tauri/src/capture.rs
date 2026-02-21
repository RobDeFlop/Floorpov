use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::{ImageEncoder, ImageEncoderPixelFormat, ImageFormat},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DrawBorderSettings, 
        DirtyRegionSettings, MinimumUpdateIntervalSettings, 
        SecondaryWindowSettings, Settings
    },
};

#[derive(Clone, serde::Serialize)]
pub struct CaptureStartedPayload {
    width: u32,
    height: u32,
    source: String,
}

#[derive(Clone, serde::Serialize)]
pub struct PreviewFramePayload {
    data: Vec<u8>,
}

struct PreviewCaptureHandler {
    app_handle: AppHandle,
    encoder: ImageEncoder,
    buffer: Vec<u8>,
    stop_rx: mpsc::Receiver<()>,
}

impl GraphicsCaptureApiHandler for PreviewCaptureHandler {
    type Flags = (AppHandle, mpsc::Receiver<()>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let encoder = ImageEncoder::new(
            ImageFormat::Jpeg, 
            ImageEncoderPixelFormat::Bgra8
        )?;
        Ok(Self { 
            app_handle: context.flags.0,
            encoder,
            buffer: Vec::new(),
            stop_rx: context.flags.1,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // Check if stop signal was received
        if self.stop_rx.try_recv().is_ok() {
            capture_control.stop();
            return Ok(());
        }

        let width = frame.width();
        let height = frame.height();

        let frame_buffer = frame.buffer()?;
        self.buffer.clear();
        let pixels = frame_buffer.as_nopadding_buffer(&mut self.buffer);

        let jpeg_bytes = self.encoder.encode(pixels, width, height)?;

        self.app_handle
            .emit("preview-frame", PreviewFramePayload { data: jpeg_bytes })?;

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.app_handle.emit("capture-stopped", ())?;
        Ok(())
    }
}

pub struct CaptureState {
    is_capturing: bool,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self { 
            is_capturing: false,
            stop_tx: None,
        }
    }
}

pub type SharedCaptureState = Arc<RwLock<CaptureState>>;

#[tauri::command]
pub async fn start_preview(
    app_handle: AppHandle,
    state: tauri::State<'_, SharedCaptureState>,
) -> Result<CaptureStartedPayload, String> {
    let mut capture_state = state.write().await;

    if capture_state.is_capturing {
        return Err("Capture already in progress".to_string());
    }

    let primary_monitor = Monitor::primary().map_err(|e| e.to_string())?;
    let width = primary_monitor.width().map_err(|e| e.to_string())?;
    let height = primary_monitor.height().map_err(|e| e.to_string())?;

    // Create channel for stop signal
    let (stop_tx, stop_rx) = mpsc::channel(1);

    capture_state.is_capturing = true;
    capture_state.stop_tx = Some(stop_tx);

    let settings = Settings::new(
        primary_monitor,
        CursorCaptureSettings::Default,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        (app_handle.clone(), stop_rx),
    );

    tokio::spawn(async move {
        if let Err(e) = PreviewCaptureHandler::start(settings) {
            eprintln!("Capture error: {}", e);
        }
    });

    Ok(CaptureStartedPayload {
        width,
        height,
        source: "Primary Monitor".to_string(),
    })
}

#[tauri::command]
pub async fn stop_preview(
    state: tauri::State<'_, SharedCaptureState>,
) -> Result<(), String> {
    let mut capture_state = state.write().await;
    
    if !capture_state.is_capturing {
        return Err("No active capture to stop".to_string());
    }

    // Send stop signal to the capture handler
    if let Some(stop_tx) = capture_state.stop_tx.take() {
        let _ = stop_tx.send(()).await;
    }

    capture_state.is_capturing = false;
    Ok(())
}

#[tauri::command]
pub async fn list_windows() -> Result<Vec<String>, String> {
    Ok(vec!["Primary Monitor".to_string()])
}
