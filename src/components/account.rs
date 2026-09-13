//! Account presentation shared by the sidebar menu and the settings pages.
//!
//! Every value here is derived from protocol fields. Missing data renders as an
//! unknown state instead of a placeholder number, and nothing about the account
//! is invented locally.

use chrono::{Datelike, Local, TimeZone, Timelike};

use crate::agent::{
    ACCOUNT_WIDE_LIMIT_ID, AGENT_DEFAULT_RATE_LIMIT_ID, AgentAccount, AgentAccountLoginPhase,
    AgentAccountPlanType, AgentAccountPresence, AgentAccountRateLimitsState, AgentAccountSnapshot,
    AgentAccountState, AgentLoginChallenge, AgentRateLimitBucket, AgentRateLimitWindow,
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
}

/// One quota window rendered as a settings row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimitRow {
    pub title: String,
    pub reset_label: Option<String>,
    pub remaining_label: Option<String>,
    /// Percent remaining, used for the row's usage bar. Unknown percentages
    /// render without a bar instead of an empty one.
    pub remaining_percent: Option<i32>,
}

/// One quota bucket rendered as a settings section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimitSection {
    pub title: String,
    pub rows: Vec<RateLimitRow>,
}

impl AccountView {
    pub fn is_loading(&self) -> bool {
        matches!(self.status, AccountLoadStatus::Loading)
    }

    pub fn load_error(&self) -> Option<&str> {
        match &self.status {
            AccountLoadStatus::Failed(message) => Some(message),
            _ => None,
        }
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

    /// Percent remaining in the bucket the account menu summarises. Unknown
    /// percentages stay unknown instead of rendering as a full quota.
    pub fn remaining_percent(&self) -> Option<i32> {
        let bucket = self.state.rate_limits.preferred_bucket()?;
        let window = bucket.primary.as_ref().or(bucket.secondary.as_ref())?;
        Some(remaining_percent(window))
    }

    pub fn quota_known(&self) -> bool {
        !self.state.rate_limits.buckets.is_empty()
    }

    pub fn usage_summary(&self) -> String {
        if self.is_loading() && !self.quota_known() {
            return "正在刷新…".to_owned();
        }
        match self.remaining_percent() {
            Some(remaining) => format!("剩余 {remaining}%"),
            None => "未知".to_owned(),
        }
    }

    pub fn reset_credits(&self) -> Option<&crate::agent::AgentRateLimitResetCredits> {
        self.state.rate_limits.reset_credits.as_ref()
    }

    /// Backend-owned upsell banner text, when the backend supplied one.
    pub fn upsell_message(&self) -> Option<String> {
        let upsell = self.state.rate_limits.upsell.as_ref()?;
        upsell
            .get("message")
            .or_else(|| upsell.get("description"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
    }

    /// Quota sections in the order the backend reported them, with the legacy
    /// single-bucket view first because that is the account-wide limit.
    pub fn rate_limit_sections(&self) -> Vec<RateLimitSection> {
        let limits = &self.state.rate_limits;
        let mut sections = Vec::new();
        if let Some(bucket) = limits.bucket(AGENT_DEFAULT_RATE_LIMIT_ID) {
            sections.push(section_for_bucket("通用使用限额", bucket));
        }
        for (key, bucket) in &limits.buckets {
            if key == AGENT_DEFAULT_RATE_LIMIT_ID {
                continue;
            }
            // The account-wide metered id reads as the general limit; named
            // buckets use the backend's own display name.
            let title = if key == ACCOUNT_WIDE_LIMIT_ID {
                "通用使用限额".to_owned()
            } else {
                match bucket
                    .limit_name
                    .as_deref()
                    .filter(|name| !name.trim().is_empty())
                {
                    Some(name) => format!("{name} 使用限额"),
                    None => format!("{key} 使用限额"),
                }
            };
            sections.push(section_for_bucket(&title, bucket));
        }
        sections
    }
}

fn section_for_bucket(title: &str, bucket: &AgentRateLimitBucket) -> RateLimitSection {
    let mut rows = Vec::new();
    if let Some(primary) = bucket.primary.as_ref() {
        rows.push(row_for_window(primary, uses_secondary_naming(bucket)));
    }
    if let Some(secondary) = bucket.secondary.as_ref() {
        rows.push(row_for_window(secondary, false));
    }
    if rows.is_empty() {
        rows.push(RateLimitRow {
            title: "使用限额".to_owned(),
            reset_label: None,
            remaining_label: None,
            remaining_percent: None,
        });
    }
    RateLimitSection {
        title: title.to_owned(),
        rows,
    }
}

/// The legacy single bucket reports one window that the backend names for the
/// long period it covers, so it keeps the product label for that window.
fn uses_secondary_naming(bucket: &AgentRateLimitBucket) -> bool {
    bucket
        .limit_id
        .as_deref()
        .is_none_or(|limit_id| limit_id == AGENT_DEFAULT_RATE_LIMIT_ID)
        && bucket.secondary.is_none()
}

fn row_for_window(window: &AgentRateLimitWindow, generic_title: bool) -> RateLimitRow {
    let remaining = remaining_percent(window);
    RateLimitRow {
        title: window_title(window.window_duration_mins, generic_title),
        reset_label: window.resets_at.map(reset_label),
        remaining_label: Some(format!("剩余 {remaining}%")),
        remaining_percent: Some(remaining),
    }
}

pub fn window_title(duration_mins: Option<i64>, generic: bool) -> String {
    match duration_mins {
        Some(300) => "5 小时使用限额".to_owned(),
        Some(1_440) => "每日使用限额".to_owned(),
        Some(10_080) => "每周使用限额".to_owned(),
        Some(minutes) => format!("{} 小时使用限额", minutes / 60),
        None if generic => "每周使用限额".to_owned(),
        None => "使用限额".to_owned(),
    }
}

pub fn remaining_percent(window: &AgentRateLimitWindow) -> i32 {
    (100 - window.used_percent).clamp(0, 100)
}

/// Reset text uses a clock time for today and an absolute date otherwise, the
/// same shape the product uses for quota windows.
pub fn reset_label(resets_at: i64) -> String {
    let Some(reset) = Local.timestamp_opt(resets_at, 0).single() else {
        return "重置时间未知".to_owned();
    };
    let now = Local::now();
    if reset.date_naive() == now.date_naive() {
        format!("重置时间：{}", reset.format("%H:%M"))
    } else {
        format!(
            "重置时间：{}年{}月{}日 {:02}:{:02}",
            reset.year(),
            reset.month(),
            reset.day(),
            reset.hour(),
            reset.minute()
        )
    }
}

/// Expiry text for a reset credit.
pub fn credit_expiry_label(expires_at: Option<i64>) -> String {
    match expires_at.and_then(|timestamp| Local.timestamp_opt(timestamp, 0).single()) {
        Some(expiry) => format!(
            "将于 {}/{} GMT+8 {:02}:{:02} 到期",
            expiry.month(),
            expiry.day(),
            expiry.hour(),
            expiry.minute()
        ),
        None => "无到期时间".to_owned(),
    }
}

/// Settings wording for the plan row. The plan type comes from the account
/// answer; an unknown tier keeps the backend's own value visible.
pub fn plan_settings_title(snapshot: &AgentAccountSnapshot) -> Option<String> {
    snapshot
        .effective_plan_type()
        .and_then(AgentAccountPlanType::settings_label)
}

/// Credits balance row, distinguishing "no credits reported" from zero.
pub fn credits_balance_label(limits: &AgentAccountRateLimitsState) -> (String, String) {
    let credits = limits
        .buckets
        .values()
        .find_map(|bucket| bucket.credits.clone());
    match credits {
        Some(credits) if credits.unlimited => ("无限制".to_owned(), "额度余额".to_owned()),
        Some(credits) => match credits.balance {
            Some(balance) if !balance.trim().is_empty() => (balance, "当前余额".to_owned()),
            _ if credits.has_credits => ("余额未知".to_owned(), "当前余额".to_owned()),
            _ => ("无可用额度".to_owned(), "当前余额".to_owned()),
        },
        None => ("额度未知".to_owned(), "当前余额".to_owned()),
    }
}
