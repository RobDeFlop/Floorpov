//! Recording backend selection and run outcomes.

pub(crate) const RECORDING_BACKEND_ENV: &str = "FLOORPOV_RECORDING_BACKEND";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordingBackendRequest {
    Ffmpeg,
    Native,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordingBackendKind {
    Ffmpeg,
    NativeWindows,
}

impl RecordingBackendKind {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::NativeWindows => "native",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordingFailurePhase {
    Startup,
    Runtime,
    Finalization,
}

#[derive(Debug)]
pub(crate) enum RecordingRunOutcome {
    Finalized {
        backend: RecordingBackendKind,
    },
    Failed {
        backend: RecordingBackendKind,
        phase: RecordingFailurePhase,
        message: String,
        startup_acknowledged: bool,
    },
    StoppedWithoutOutput {
        backend: RecordingBackendKind,
        message: Option<String>,
        startup_acknowledged: bool,
    },
}

impl RecordingRunOutcome {
    pub(crate) const fn backend(&self) -> RecordingBackendKind {
        match self {
            Self::Finalized { backend }
            | Self::Failed { backend, .. }
            | Self::StoppedWithoutOutput { backend, .. } => *backend,
        }
    }

    pub(crate) const fn startup_acknowledged(&self) -> bool {
        match self {
            Self::Finalized { .. } => true,
            Self::Failed {
                startup_acknowledged,
                ..
            }
            | Self::StoppedWithoutOutput {
                startup_acknowledged,
                ..
            } => *startup_acknowledged,
        }
    }
}

pub(crate) fn parse_backend_value(value: Option<&str>) -> RecordingBackendRequest {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("native") => RecordingBackendRequest::Native,
        Some("auto") => RecordingBackendRequest::Auto,
        Some("ffmpeg") | None | Some("") => RecordingBackendRequest::Ffmpeg,
        Some(invalid) => {
            tracing::warn!(
                value = invalid,
                "Invalid recording backend selection; using FFmpeg"
            );
            RecordingBackendRequest::Ffmpeg
        }
    }
}

pub(crate) fn requested_backend() -> RecordingBackendRequest {
    let value = std::env::var(RECORDING_BACKEND_ENV).ok();
    parse_backend_value(value.as_deref())
}

pub(crate) fn initial_backend(
    request: RecordingBackendRequest,
    video_encoder_preference: &str,
) -> RecordingBackendKind {
    match request {
        RecordingBackendRequest::Ffmpeg => RecordingBackendKind::Ffmpeg,
        RecordingBackendRequest::Native => RecordingBackendKind::NativeWindows,
        RecordingBackendRequest::Auto if video_encoder_preference != "auto" => {
            RecordingBackendKind::Ffmpeg
        }
        RecordingBackendRequest::Auto => RecordingBackendKind::NativeWindows,
    }
}

pub(crate) fn should_fallback_to_ffmpeg(
    request: RecordingBackendRequest,
    outcome: &RecordingRunOutcome,
    stop_requested: bool,
) -> bool {
    matches!(request, RecordingBackendRequest::Auto)
        && !stop_requested
        && !outcome.startup_acknowledged()
        && matches!(
            outcome,
            RecordingRunOutcome::Failed {
                backend: RecordingBackendKind::NativeWindows,
                phase: RecordingFailurePhase::Startup,
                ..
            }
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_startup_failure(startup_acknowledged: bool) -> RecordingRunOutcome {
        RecordingRunOutcome::Failed {
            backend: RecordingBackendKind::NativeWindows,
            phase: RecordingFailurePhase::Startup,
            message: "startup failed".to_string(),
            startup_acknowledged,
        }
    }

    #[test]
    fn parses_backend_environment_values() {
        assert_eq!(parse_backend_value(None), RecordingBackendRequest::Ffmpeg);
        assert_eq!(
            parse_backend_value(Some("ffmpeg")),
            RecordingBackendRequest::Ffmpeg
        );
        assert_eq!(
            parse_backend_value(Some(" NATIVE ")),
            RecordingBackendRequest::Native
        );
        assert_eq!(
            parse_backend_value(Some("auto")),
            RecordingBackendRequest::Auto
        );
        assert_eq!(
            parse_backend_value(Some("invalid")),
            RecordingBackendRequest::Ffmpeg
        );
    }

    #[test]
    fn explicit_encoder_preference_keeps_auto_on_ffmpeg() {
        assert_eq!(
            initial_backend(RecordingBackendRequest::Auto, "h264_nvenc"),
            RecordingBackendKind::Ffmpeg
        );
        assert_eq!(
            initial_backend(RecordingBackendRequest::Auto, "auto"),
            RecordingBackendKind::NativeWindows
        );
    }

    #[test]
    fn forced_backend_selection_is_deterministic() {
        assert_eq!(
            initial_backend(RecordingBackendRequest::Native, "h264_nvenc"),
            RecordingBackendKind::NativeWindows
        );
        assert_eq!(
            initial_backend(RecordingBackendRequest::Ffmpeg, "auto"),
            RecordingBackendKind::Ffmpeg
        );
    }

    #[test]
    fn fallback_is_limited_to_unacknowledged_native_startup_failure() {
        assert!(should_fallback_to_ffmpeg(
            RecordingBackendRequest::Auto,
            &native_startup_failure(false),
            false
        ));
        assert!(!should_fallback_to_ffmpeg(
            RecordingBackendRequest::Native,
            &native_startup_failure(false),
            false
        ));
        assert!(!should_fallback_to_ffmpeg(
            RecordingBackendRequest::Auto,
            &native_startup_failure(true),
            false
        ));
        assert!(!should_fallback_to_ffmpeg(
            RecordingBackendRequest::Auto,
            &native_startup_failure(false),
            true
        ));
        assert!(!should_fallback_to_ffmpeg(
            RecordingBackendRequest::Auto,
            &RecordingRunOutcome::Failed {
                backend: RecordingBackendKind::NativeWindows,
                phase: RecordingFailurePhase::Runtime,
                message: "runtime failed".to_string(),
                startup_acknowledged: true,
            },
            false
        ));
        assert!(!should_fallback_to_ffmpeg(
            RecordingBackendRequest::Auto,
            &RecordingRunOutcome::StoppedWithoutOutput {
                backend: RecordingBackendKind::NativeWindows,
                message: None,
                startup_acknowledged: false,
            },
            false
        ));
    }
}
