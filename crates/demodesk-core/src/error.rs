//! Application-wide error codes shared by APIs, persisted jobs and events.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    AppClosed,
    Cs2AlreadyRunning,
    ToolsMissing,
    DemoMissing,
    DemoNotReady,
    DemoPathUnsupported,
    NoHighlights,
    CameraUnavailable,
    GameLaunchFailed,
    GameExited,
    HlaeUnavailable,
    ConsoleUnavailable,
    DemoStartFailed,
    RecordingTimeout,
    RecordingFailed,
    WindowHookFailed,
    NoVideoRecorded,
    MergeIncomplete,
    EncoderUnavailable,
    GpuUnavailable,
    EncodingFailed,
    EncodingStalled,
    InvalidMedia,
    InvalidExportOptions,
    SizeLimitTooSmall,
    SizeLimitUnreachable,
    DiskFull,
    PermissionDenied,
    FileBusy,
    FileUnavailable,
    ExportFailed,
    ExportCancelled,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AppClosed => "Job interrupted because the app was closed",
            Self::Cs2AlreadyRunning => "cs2.exe is already running — close the game first",
            Self::ToolsMissing => "Recording tools are missing. Check Settings.",
            Self::DemoMissing => "Demo file not found. Add it again.",
            Self::DemoNotReady => "Demo data is unavailable or outdated. Analyze it again.",
            Self::DemoPathUnsupported => {
                "Move the Demo to a path containing only English letters, numbers and symbols."
            }
            Self::NoHighlights => "Select at least one clip.",
            Self::CameraUnavailable => "Player view is unavailable. Analyze the Demo again.",
            Self::GameLaunchFailed => "CS2 could not start. Check Steam and HLAE.",
            Self::GameExited => "CS2 closed before recording finished.",
            Self::HlaeUnavailable => "HLAE could not connect to CS2. Update HLAE.",
            Self::ConsoleUnavailable => "Could not communicate with CS2. Close the game and retry.",
            Self::DemoStartFailed => "Demo playback did not start. Check that CS2 can play it.",
            Self::RecordingTimeout => "Recording timed out. Please retry.",
            Self::RecordingFailed => "Recording failed. See the log for details.",
            Self::WindowHookFailed => {
                "Background recording could not start. Try showing the game window."
            }
            Self::NoVideoRecorded => "No video was produced. Check HLAE and FFmpeg.",
            Self::MergeIncomplete => "Some clips are incomplete and cannot be merged.",
            Self::EncoderUnavailable => "FFmpeg lacks a required feature. Download FFmpeg again.",
            Self::GpuUnavailable => "GPU encoding is unavailable. Check the driver or choose CPU.",
            Self::EncodingFailed => "Video processing failed. See the log for details.",
            Self::EncodingStalled => "Video processing stalled. Please retry.",
            Self::InvalidMedia => "The video is incomplete or unreadable. Record it again.",
            Self::InvalidExportOptions => "Export settings are invalid. Check the options.",
            Self::SizeLimitTooSmall => {
                "The size limit is too small. Increase it or shorten the clip."
            }
            Self::SizeLimitUnreachable => {
                "Could not meet the size limit. Increase it or shorten the clip."
            }
            Self::DiskFull => "Not enough disk space. Free up space and retry.",
            Self::PermissionDenied => {
                "Folder access denied. Check permissions or change the data folder."
            }
            Self::FileBusy => "The file is in use. Close the program using it and retry.",
            Self::FileUnavailable => "A required file is missing or unreadable. Check the path.",
            Self::ExportFailed => "Export failed. See the log for details.",
            Self::ExportCancelled => "cancelled",
        })
    }
}

impl std::error::Error for ErrorCode {}

impl ErrorCode {
    pub fn classify(error: &anyhow::Error, fallback: Self) -> Self {
        if let Some(io) = error.downcast_ref::<std::io::Error>() {
            match io.kind() {
                std::io::ErrorKind::StorageFull => return Self::DiskFull,
                std::io::ErrorKind::PermissionDenied => return Self::PermissionDenied,
                _ => {}
            }
            #[cfg(windows)]
            if matches!(io.raw_os_error(), Some(32 | 33)) {
                return Self::FileBusy;
            }
        }
        if let Some(code) = error.downcast_ref::<Self>() {
            return *code;
        }
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Self::FileUnavailable;
        }
        fallback
    }

    pub fn wrap(self, error: anyhow::Error) -> anyhow::Error {
        let code = Self::classify(&error, self);
        error.context(code)
    }

    // FFmpeg reports failures on stderr rather than through structured exit codes.
    pub fn ffmpeg(stderr: &str) -> Self {
        let text = stderr.to_ascii_lowercase();
        if text.contains("no space left on device") || text.contains("disk full") {
            Self::DiskFull
        } else if text.contains("permission denied") || text.contains("access is denied") {
            Self::PermissionDenied
        } else if [
            "no capable devices",
            "cannot load nvcuda",
            "cannot load nvencodeapi",
            "driver does not support",
            "minimum required nvidia driver",
            "unsupported device",
        ]
        .iter()
        .any(|s| text.contains(s))
        {
            Self::GpuUnavailable
        } else if [
            "unknown encoder",
            "encoder not found",
            "no such filter",
            "filter not found",
        ]
        .iter()
        .any(|s| text.contains(s))
        {
            Self::EncoderUnavailable
        } else if text.contains("invalid data found when processing input")
            || text.contains("moov atom not found")
        {
            Self::InvalidMedia
        } else {
            Self::EncodingFailed
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub error_code: ErrorCode,
    pub message: String,
}

impl ExportError {
    pub fn new(error: &anyhow::Error, fallback: ErrorCode) -> Self {
        Self {
            error_code: ErrorCode::classify(error, fallback),
            message: format!("{error:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_keep_specific_codes_and_diagnostics() {
        use std::io::{Error, ErrorKind};
        for (kind, code) in [
            (ErrorKind::StorageFull, ErrorCode::DiskFull),
            (ErrorKind::PermissionDenied, ErrorCode::PermissionDenied),
            (ErrorKind::NotFound, ErrorCode::FileUnavailable),
        ] {
            let error =
                ErrorCode::RecordingFailed.wrap(Error::new(kind, "original diagnostic").into());
            let payload = ExportError::new(&error, ErrorCode::ExportFailed);
            assert_eq!(payload.error_code, code);
            assert!(payload.message.contains("original diagnostic"));
            assert_eq!(
                serde_json::to_value(&payload).unwrap()["errorCode"],
                serde_json::to_value(code).unwrap()
            );
        }
        for code in [
            ErrorCode::Cs2AlreadyRunning,
            ErrorCode::ExportCancelled,
            ErrorCode::EncodingStalled,
        ] {
            let error = ErrorCode::RecordingFailed.wrap(anyhow::Error::new(code).context("stage"));
            assert_eq!(ErrorCode::classify(&error, ErrorCode::ExportFailed), code);
        }
        assert_eq!(
            ErrorCode::classify(&anyhow::anyhow!("unexpected"), ErrorCode::ExportFailed),
            ErrorCode::ExportFailed
        );
        #[cfg(windows)]
        assert_eq!(
            ErrorCode::classify(
                &Error::from_raw_os_error(32).into(),
                ErrorCode::ExportFailed
            ),
            ErrorCode::FileBusy
        );
    }

    #[test]
    fn ffmpeg_errors_are_classified_without_exposing_stderr_as_ui_text() {
        for (diagnostic, code) in [
            ("No space left on device", ErrorCode::DiskFull),
            ("Permission denied", ErrorCode::PermissionDenied),
            ("Cannot load nvEncodeAPI64.dll", ErrorCode::GpuUnavailable),
            ("Unknown encoder 'libsvtav1'", ErrorCode::EncoderUnavailable),
            ("No such filter: setsar", ErrorCode::EncoderUnavailable),
            ("moov atom not found", ErrorCode::InvalidMedia),
            ("unexpected exit", ErrorCode::EncodingFailed),
        ] {
            assert_eq!(ErrorCode::ffmpeg(diagnostic), code);
        }
    }

    #[test]
    fn recording_error_code_survives_context_and_serialization() {
        let error =
            anyhow::Error::new(ErrorCode::Cs2AlreadyRunning).context("recording could not start");
        let code = error.downcast_ref::<ErrorCode>().copied();
        assert_eq!(code, Some(ErrorCode::Cs2AlreadyRunning));
        assert_eq!(serde_json::to_value(code).unwrap(), "cs2-already-running");
        assert_eq!(
            serde_json::from_str::<ErrorCode>("\"cs2-already-running\"").unwrap(),
            ErrorCode::Cs2AlreadyRunning
        );
    }
}
