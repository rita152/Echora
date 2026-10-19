//! Image preview behavior and presentation for the application shell.

use std::{path::PathBuf, process::Command};

use gpui::Context;

use super::ChatApp;

impl ChatApp {
    pub(super) fn annotate_preview_image(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.image_preview.path.clone() else {
            return;
        };
        let Some(composer) = self
            .image_preview
            .composer
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
        else {
            return;
        };
        if composer.update(cx, |composer, cx| composer.annotate_attachment(&path, cx)) {
            self.image_preview.path = None;
            self.image_preview.composer = None;
            // The markup editor now owns focus; do not restore the preview's prompt.
            self.image_preview.previous_focus = None;
            self.image_preview.focus_active = false;
        }
        cx.notify();
    }

    pub(super) fn download_preview_image(&mut self, source: PathBuf, cx: &mut Context<Self>) {
        let suggested_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image.png")
            .to_owned();
        let initial_directory = source
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let destination = cx.prompt_for_new_path(&initial_directory, Some(suggested_name.as_str()));
        cx.spawn(async move |_, _| {
            let Ok(Ok(Some(destination))) = destination.await else {
                return;
            };
            let _ = std::fs::copy(source, destination);
        })
        .detach();
    }
}

pub(super) fn finder_reveal_command(path: &str) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}
