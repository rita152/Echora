//! Native window observation for Computer Use. Only the application selected
//! by tool-surface metadata is observed; this never captures the whole desktop.

#[cfg(target_os = "macos")]
mod macos;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    AppId(String),
    DisplayName(String),
}

impl Target {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::AppId(value) | Self::DisplayName(value) => value,
        }
    }
}

pub(crate) struct Frame {
    pub png: Vec<u8>,
    pub title: String,
    pub application: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureError {
    PermissionRequired,
    WindowUnavailable,
    CaptureFailed,
    Unsupported,
}

pub(crate) fn capture(target: Target) -> async_channel::Receiver<Result<Frame, CaptureError>> {
    #[cfg(target_os = "macos")]
    {
        macos::capture(target)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(CaptureError::Unsupported));
        receiver
    }
}

/// Focus an already running target after the user explicitly asks to intervene.
pub(crate) fn focus(target: &Target) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::focus(target)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        false
    }
}
