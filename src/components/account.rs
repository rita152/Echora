//! Account presentation for the sidebar account menu and its dialogs.
//!
//! Every value here is derived from protocol fields. Missing data renders as an
//! unknown state instead of a placeholder number, and nothing about the account
//! is invented locally.

use crate::agent::{
    AgentAccount, AgentAccountLoginPhase, AgentAccountPlanType, AgentAccountPresence,
    AgentAccountState, AgentLoginChallenge,
};

/// Dialog owned by the account surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountDialog {
    /// Confirmation shown before the logout RPC is sent.
    Logout,
    /// Login progress, challenge, and retry.
    Login,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AccountLoadStatus {
    #[default]
    Idle,
    Loading,
    Loaded,
    Failed(String),
}

/// Account surfaces the views render, including the in-flight operation state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountView {
    pub state: AgentAccountState,
    pub status: AccountLoadStatus,
    pub dialog: Option<AccountDialog>,
    /// Failure of the last explicit account action (login, cancel, logout).
    pub action_error: Option<String>,
    /// Display name of the effective `model_provider` from `config/read`.
    pub model_provider_name: Option<String>,
}

impl AccountView {
    pub fn is_loading(&self) -> bool {
        matches!(self.status, AccountLoadStatus::Loading)
    }

    /// True only when the backend actually reported that this client has no
    /// account. A connection that has not answered yet stays unknown instead of
    /// being presented as signed out.
    pub fn signed_out(&self) -> bool {
        if self.account_unknown() {
            return false;
        }
        match &self.state.account.account {
            AgentAccountPresence::Null => true,
            AgentAccountPresence::Missing => !self.state.account.requires_openai_auth,
            AgentAccountPresence::Account(_) => false,
        }
    }

    pub fn needs_login(&self) -> bool {
        self.signed_out()
    }

    /// True until an account answer has been received on this connection. The
    /// default snapshot is not an answer: it never renders as signed out.
    pub fn account_unknown(&self) -> bool {
        matches!(self.state.account.account, AgentAccountPresence::Missing)
            && !matches!(self.status, AccountLoadStatus::Loaded)
    }

    pub fn is_signed_in(&self) -> bool {
        matches!(
            self.state.account.account,
            AgentAccountPresence::Account(AgentAccount::Chatgpt { .. })
        )
    }

    /// Footer identity when the connection is not a ChatGPT sign-in: the
    /// desktop app then labels the entry with the configured provider's name
    /// and a settings glyph instead of an avatar.
    pub fn footer_provider_label(&self) -> Option<&str> {
        if self.is_signed_in() {
            return None;
        }
        self.model_provider_name.as_deref()
    }

    /// Account label for the menu header: the local part of the reported email.
    /// It is derived from the backend answer, never from a stored profile.
    pub fn account_label(&self) -> Option<String> {
        let email = self.state.account.email()?;
        let local = email.split('@').next().unwrap_or(email);
        (!local.is_empty()).then(|| local.to_owned())
    }

    pub fn account_email(&self) -> Option<&str> {
        self.state.account.email()
    }

    /// Avatar initials derived from the account label.
    pub fn account_initials(&self) -> Option<String> {
        let label = self.account_label()?;
        let initials: String = label
            .split(|character: char| !character.is_alphanumeric())
            .filter(|part| !part.is_empty())
            .take(2)
            .filter_map(|part| part.chars().next())
            .collect();
        Some(initials.to_uppercase())
    }

    pub fn plan_label(&self) -> Option<&'static str> {
        self.state
            .account
            .effective_plan_type()
            .and_then(AgentAccountPlanType::short_label)
    }

    pub fn login_pending(&self) -> bool {
        self.state.login.phase == AgentAccountLoginPhase::InProgress
    }

    pub fn login_id(&self) -> Option<&str> {
        self.state.login.login_id.as_deref()
    }

    pub fn challenge(&self) -> Option<&AgentLoginChallenge> {
        self.state.login.challenge.as_ref()
    }

    pub fn login_error(&self) -> Option<&str> {
        self.state.login.error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentAccountPlanType, AgentAccountSnapshot};

    fn view(account: AgentAccountPresence) -> AccountView {
        let mut view = AccountView {
            model_provider_name: Some("deepseek".into()),
            status: AccountLoadStatus::Loaded,
            ..AccountView::default()
        };
        view.state.account = AgentAccountSnapshot {
            account,
            ..AgentAccountSnapshot::default()
        };
        view
    }

    #[test]
    fn footer_names_the_configured_provider_unless_signed_in_with_chatgpt() {
        assert_eq!(
            view(AgentAccountPresence::Null).footer_provider_label(),
            Some("deepseek")
        );
        assert_eq!(
            view(AgentAccountPresence::Account(AgentAccount::ApiKey)).footer_provider_label(),
            Some("deepseek")
        );
        let chatgpt = view(AgentAccountPresence::Account(AgentAccount::Chatgpt {
            email: Some("rita@example.com".into()),
            plan_type: AgentAccountPlanType::Pro,
        }));
        assert_eq!(chatgpt.footer_provider_label(), None);
        let unconfigured = AccountView {
            model_provider_name: None,
            ..view(AgentAccountPresence::Null)
        };
        assert_eq!(unconfigured.footer_provider_label(), None);
    }
}
