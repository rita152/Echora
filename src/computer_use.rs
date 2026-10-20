//! Native window identities and continuous, bounded Computer Use video streams.
//! Only applications selected by tool-surface metadata are observed.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::Stream;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// Local identity; the MCP application metadata does not supply window IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WindowId {
    pub process: i32,
    pub window: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowInfo {
    pub id: WindowId,
    pub target: Target,
    pub title: String,
    pub application: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureError {
    PermissionRequired,
    AccessibilityRequired,
    WindowUnavailable,
    CaptureFailed,
    Unsupported,
}

pub(crate) const MAX_WINDOWS: usize = 5;

/// Enumerate matching windows, frontmost first. This does not capture images.
pub(crate) fn windows(
    targets: Vec<Target>,
) -> async_channel::Receiver<Result<Vec<WindowInfo>, CaptureError>> {
    #[cfg(target_os = "macos")]
    {
        macos::windows(targets)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = targets;
        let (sender, receiver) = async_channel::bounded(1);
        let _ = sender.try_send(Err(CaptureError::Unsupported));
        receiver
    }
}

/// Focus the exact window only after the agent has confirmed it stopped.
pub(crate) fn focus(window: &WindowInfo) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::focus(window)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        false
    }
}
