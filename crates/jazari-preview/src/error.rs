//! The crate's error type.
//!
//! The public API returns `anyhow::Result` (see `docs/ARCHITECTURE.md`,
//! section 9), but every error it creates is a [`PreviewError`], so callers
//! can `downcast_ref::<PreviewError>()` to tell the cases apart.

/// Something the preview could not do.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    /// No software adapter exists (WARP needs Windows 10 or later; on Linux
    /// lavapipe comes with `mesa-vulkan-drivers`).
    #[error(
        "no software adapter (WARP on Windows, lavapipe or llvmpipe on Linux) is available; adapters found: {found}"
    )]
    NoSoftwareAdapter {
        /// A summary of the adapters that were enumerated.
        found: String,
    },

    /// The software adapter refused to create a device.
    #[error("could not create the wgpu device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    /// The bytes are not a playable SWF.
    #[error("not a playable SWF: {0}")]
    InvalidMovie(String),

    /// Ruffle could not create its renderer (for example a texture larger
    /// than the device allows).
    #[error("Ruffle could not create its renderer: {0}")]
    Renderer(String),

    /// A frame number of 0 was asked for; frames are 1-based.
    #[error("frame numbers are 1-based; frame 0 was requested")]
    FrameZero,

    /// Ruffle panicked; the message is the panic payload.
    #[error("Ruffle panicked while {stage}: {message}")]
    Panicked {
        /// What the preview was doing.
        stage: &'static str,
        /// The panic message.
        message: String,
    },

    /// A rendered frame could not be read back.
    #[error("frame {frame} could not be captured")]
    Capture {
        /// The 1-based frame.
        frame: u32,
    },
}

/// The text of a panic payload (`&str` or `String`), or a placeholder.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_messages_are_extracted() {
        let payload = std::panic::catch_unwind(|| panic!("boom {}", 1)).unwrap_err();
        assert_eq!(panic_message(&*payload), "boom 1");
        let payload = std::panic::catch_unwind(|| panic!("static")).unwrap_err();
        assert_eq!(panic_message(&*payload), "static");
    }
}
