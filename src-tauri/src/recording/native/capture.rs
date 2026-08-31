//! Windows Graphics Capture integration for the native encoder.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::VideoEncoder;
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use super::timing::{FrameGate, QpcClock, TimestampReservation};

#[derive(Debug)]
pub(super) enum CaptureEvent {
    FirstFrame,
    Closed,
}

#[derive(Default)]
pub(super) struct CaptureStats {
    pub(super) captured_frames: AtomicU64,
    pub(super) accepted_frames: AtomicU64,
    pub(super) dropped_frames: AtomicU64,
    pub(super) submission_failures: AtomicU64,
}

#[derive(Clone)]
pub(super) struct CaptureShared {
    pub(super) encoder: Arc<Mutex<Option<VideoEncoder>>>,
    pub(super) submission_lock: Arc<Mutex<()>>,
    pub(super) timestamps: Arc<Mutex<TimestampReservation>>,
    pub(super) frame_gate: Arc<Mutex<FrameGate>>,
    pub(super) clock: QpcClock,
    pub(super) output_width: u32,
    pub(super) output_height: u32,
    pub(super) enable_diagnostics: bool,
    pub(super) stats: Arc<CaptureStats>,
}

#[derive(Clone)]
pub(super) struct CaptureFlags {
    shared: CaptureShared,
    event_tx: mpsc::Sender<CaptureEvent>,
}

pub(super) struct NativeCaptureHandler {
    flags: CaptureFlags,
    first_frame_sent: bool,
    dimension_warning_sent: bool,
    clock_delta_logged: bool,
}

impl GraphicsCaptureApiHandler for NativeCaptureHandler {
    type Flags = CaptureFlags;
    type Error = String;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            flags: context.flags,
            first_frame_sent: false,
            dimension_warning_sent: false,
            clock_delta_logged: false,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        self.flags
            .shared
            .stats
            .captured_frames
            .fetch_add(1, Ordering::Relaxed);
        let timestamp = frame
            .timestamp()
            .map_err(|error| format!("Failed to read WGC frame timestamp: {error}"))?
            .Duration;

        if !self.clock_delta_logged && self.flags.shared.enable_diagnostics {
            if let Ok(now) = self.flags.shared.clock.now_100ns() {
                tracing::info!(
                    backend = "native",
                    wgc_qpc_delta_100ns = timestamp.saturating_sub(now),
                    "Observed initial WGC-to-QPC timestamp delta"
                );
            }
            self.clock_delta_logged = true;
        }

        if (frame.width() != self.flags.shared.output_width
            || frame.height() != self.flags.shared.output_height)
            && !self.dimension_warning_sent
        {
            tracing::warn!(
                backend = "native",
                source_width = frame.width(),
                source_height = frame.height(),
                output_width = self.flags.shared.output_width,
                output_height = self.flags.shared.output_height,
                "Native capture source dimensions changed; output remains fixed and may be padded or cropped"
            );
            self.dimension_warning_sent = true;
        }

        let accepted_by_gate = self
            .flags
            .shared
            .frame_gate
            .lock()
            .map_err(|_| "Native frame gate lock was poisoned".to_string())?
            .accept(timestamp);
        if !accepted_by_gate {
            self.flags
                .shared
                .stats
                .dropped_frames
                .fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let _submission_guard = self
            .flags
            .shared
            .submission_lock
            .lock()
            .map_err(|_| "Native submission lock was poisoned".to_string())?;
        let reserved = self
            .flags
            .shared
            .timestamps
            .lock()
            .map_err(|_| "Native timestamp lock was poisoned".to_string())?
            .reserve(timestamp);
        if reserved.is_none() {
            self.flags
                .shared
                .stats
                .dropped_frames
                .fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let mut encoder_guard = self
            .flags
            .shared
            .encoder
            .lock()
            .map_err(|_| "Native encoder lock was poisoned".to_string())?;
        let encoder = encoder_guard
            .as_mut()
            .ok_or_else(|| "Native encoder closed while capture was active".to_string())?;
        if let Err(error) = encoder.send_frame(frame) {
            self.flags
                .shared
                .stats
                .submission_failures
                .fetch_add(1, Ordering::Relaxed);
            return Err(format!("Failed to submit WGC frame: {error}"));
        }
        self.flags
            .shared
            .stats
            .accepted_frames
            .fetch_add(1, Ordering::Relaxed);

        if !self.first_frame_sent {
            self.flags
                .event_tx
                .send(CaptureEvent::FirstFrame)
                .map_err(|error| format!("Failed to publish first WGC frame: {error}"))?;
            self.first_frame_sent = true;
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.flags
            .event_tx
            .send(CaptureEvent::Closed)
            .map_err(|error| format!("Failed to publish WGC closure: {error}"))
    }
}

pub(super) type NativeCaptureControl = CaptureControl<NativeCaptureHandler, String>;

fn capture_settings<T>(
    target: T,
    shared: CaptureShared,
    event_tx: mpsc::Sender<CaptureEvent>,
    frame_rate: u32,
) -> Settings<CaptureFlags, T>
where
    T: TryInto<windows_capture::settings::GraphicsCaptureItemType>,
{
    Settings::new(
        target,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Exclude,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(
            1.0 / f64::from(frame_rate.max(1)),
        )),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        CaptureFlags { shared, event_tx },
    )
}

pub(super) fn start_monitor_capture(
    shared: CaptureShared,
    frame_rate: u32,
) -> Result<(NativeCaptureControl, mpsc::Receiver<CaptureEvent>), String> {
    let monitor = Monitor::primary()
        .map_err(|error| format!("Failed to resolve primary monitor: {error}"))?;
    let (event_tx, event_rx) = mpsc::channel();
    let control = NativeCaptureHandler::start_free_threaded(capture_settings(
        monitor, shared, event_tx, frame_rate,
    ))
    .map_err(|error| format!("Failed to start native primary monitor capture: {error}"))?;
    Ok((control, event_rx))
}
