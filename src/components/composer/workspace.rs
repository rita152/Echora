//! The project and checkout the composer's workspace controls and the home
//! heading name for the conversation's working directory.

use gpui::{Context, SharedString};

use super::ComposerView;
use crate::git_review::Checkout;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspacePresentation {
    /// `None` for a projectless conversation, which the reference neither
    /// names in its heading nor attaches to a checkout.
    pub project_label: Option<SharedString>,
    /// `None` until the working directory's checkout has been resolved.
    pub checkout: Option<Checkout>,
}

impl ComposerView {
    pub fn workspace_presentation(&self) -> &WorkspacePresentation {
        &self.workspace
    }

    /// The host resolves the label from the workspace list, so a renamed
    /// project reaches the heading without the conversation being rebuilt.
    pub fn set_project_label(&mut self, label: Option<SharedString>, cx: &mut Context<Self>) {
        if self.workspace.project_label != label {
            self.workspace.project_label = label;
            cx.notify();
        }
    }

    /// Re-reads the checkout whenever the conversation is shown, since the
    /// branch can change outside the app. A known checkout stays on screen
    /// until the new answer arrives, unless the directory itself changed.
    pub(super) fn refresh_checkout(&mut self, cwd_changed: bool, cx: &mut Context<Self>) {
        self.checkout_cycle = self.checkout_cycle.wrapping_add(1);
        if cwd_changed || self.conversation.project_id.is_none() {
            self.workspace.checkout = None;
        }
        if self.conversation.project_id.is_none() {
            return;
        }
        #[cfg(not(test))]
        {
            let cycle = self.checkout_cycle;
            let cwd = self.conversation.cwd.clone();
            cx.spawn(async move |this, cx| {
                let lookup = cwd.clone();
                let checkout = cx
                    .background_executor()
                    .spawn(async move { crate::git_review::checkout(&lookup) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.checkout_cycle == cycle && this.conversation.cwd == cwd {
                        this.set_checkout(checkout, cx);
                    }
                });
            })
            .detach();
        }
        #[cfg(test)]
        let _ = cx;
    }

    pub(crate) fn set_checkout(&mut self, checkout: Checkout, cx: &mut Context<Self>) {
        if self.workspace.checkout.as_ref() != Some(&checkout) {
            self.workspace.checkout = Some(checkout);
            cx.notify();
        }
    }
}
