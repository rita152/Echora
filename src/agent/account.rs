//! Connection-scoped account, login, and quota state.
//!
//! Every value here maps to one protocol field. Missing or null backend values
//! stay missing: nothing becomes zero, an empty string, or a derived default,
//! and a nullable field in a rolling update never clears a confirmed value.

use std::collections::BTreeMap;

/// Client-side bucket key for snapshots whose backend payload omits limitId.
/// It matches the historical single-bucket format and never collides with a
/// metered backend id such as "codex".
pub const AGENT_DEFAULT_RATE_LIMIT_ID: &str = "default";

/// The account-wide metered limit the backend reports for the shared quota.
/// It renders as the general usage limit rather than a model-specific one.
pub const ACCOUNT_WIDE_LIMIT_ID: &str = "codex";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAccountPlanType {
    Free,
    Go,
    Plus,
    Pro,
    Prolite,
    Team,
    SelfServeBusinessProlite,
    SelfServeBusinessUsageBased,
    Business,
    Ent26,
    EnterpriseCbpAutomation,
    EnterpriseCbpUsageBased,
    Enterprise,
    Edu,
    EduPlus,
    EduPro,
    Unknown,
}

impl AgentAccountPlanType {
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "free" => Self::Free,
            "go" => Self::Go,
            "plus" => Self::Plus,
            "pro" => Self::Pro,
            "prolite" => Self::Prolite,
            "team" => Self::Team,
            "self_serve_business_prolite" => Self::SelfServeBusinessProlite,
            "self_serve_business_usage_based" => Self::SelfServeBusinessUsageBased,
            "business" => Self::Business,
            "ent26" => Self::Ent26,
            "enterprise_cbp_automation" => Self::EnterpriseCbpAutomation,
            "enterprise_cbp_usage_based" => Self::EnterpriseCbpUsageBased,
            "enterprise" => Self::Enterprise,
            "edu" => Self::Edu,
            "edu_plus" => Self::EduPlus,
            "edu_pro" => Self::EduPro,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    /// Short label used by the account menu. Unknown has no product label.
    pub fn short_label(self) -> Option<&'static str> {
        Some(match self {
            Self::Free => "Free",
            Self::Go => "Go",
            Self::Plus => "Plus",
            Self::Pro => "Pro",
            Self::Prolite => "Prolite",
            Self::Team => "Team",
            Self::SelfServeBusinessProlite | Self::SelfServeBusinessUsageBased | Self::Business => {
                "Business"
            }
            Self::Ent26
            | Self::EnterpriseCbpAutomation
            | Self::EnterpriseCbpUsageBased
            | Self::Enterprise => "Enterprise",
            Self::Edu | Self::EduPlus | Self::EduPro => "Edu",
            Self::Unknown => return None,
        })
    }

    /// Plan row title used by the billing settings page.
    pub fn settings_label(self) -> Option<String> {
        self.short_label().map(|label| format!("{label} 套餐"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAccountAuthMode {
    ApiKey,
    Chatgpt,
    ChatgptAuthTokens,
    Headers,
    AgentIdentity,
    PersonalAccessToken,
    BedrockApiKey,
    BedrockAccessKeys,
}

impl AgentAccountAuthMode {
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "apikey" => Self::ApiKey,
            "chatgpt" => Self::Chatgpt,
            "chatgptAuthTokens" => Self::ChatgptAuthTokens,
            "headers" => Self::Headers,
            "agentIdentity" => Self::AgentIdentity,
            "personalAccessToken" => Self::PersonalAccessToken,
            "bedrockApiKey" => Self::BedrockApiKey,
            "bedrockAccessKeys" => Self::BedrockAccessKeys,
            _ => return None,
        })
    }
}

/// Account variants reported by account/read. Variants this phase does not
/// surface keep their protocol identity so the UI can state what is connected
/// instead of pretending the user is signed out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentAccount {
    ApiKey,
    Chatgpt {
        email: Option<String>,
        plan_type: AgentAccountPlanType,
    },
    AmazonBedrock {
        uses_codex_managed_credentials: bool,
    },
}

/// account = null and a missing account field are different answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AgentAccountPresence {
    #[default]
    Missing,
    Null,
    Account(AgentAccount),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountSnapshot {
    pub requires_openai_auth: bool,
    pub account: AgentAccountPresence,
    /// Nullable authMode from account/updated; None means unavailable.
    pub auth_mode: Option<AgentAccountAuthMode>,
    /// Nullable planType from account/updated; None means unavailable.
    pub plan_type: Option<AgentAccountPlanType>,
}

impl AgentAccountSnapshot {
    /// Plan reported by the account itself, falling back to the connection
    /// level planType notification. Never invented.
    pub fn effective_plan_type(&self) -> Option<AgentAccountPlanType> {
        match &self.account {
            AgentAccountPresence::Account(AgentAccount::Chatgpt { plan_type, .. }) => {
                Some(*plan_type)
            }
            _ => self.plan_type,
        }
    }

    pub fn email(&self) -> Option<&str> {
        match &self.account {
            AgentAccountPresence::Account(AgentAccount::Chatgpt { email, .. }) => email.as_deref(),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentAccountLoginPhase {
    /// No login is in flight and no completed login is recorded on this
    /// connection generation.
    #[default]
    SignedOut,
    InProgress,
    SignedIn,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentLoginChallenge {
    /// chatgpt login: the URL the user opens in a browser.
    AuthUrl { auth_url: String },
    /// chatgptDeviceCode login: the URL plus the one-time user code.
    DeviceCode {
        verification_url: String,
        user_code: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountLoginState {
    pub phase: AgentAccountLoginPhase,
    pub login_id: Option<String>,
    pub challenge: Option<AgentLoginChallenge>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLoginStart {
    pub login_id: String,
    pub challenge: AgentLoginChallenge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentLoginCancelOutcome {
    /// The backend canceled the login this client was waiting on.
    Canceled,
    /// The backend no longer knows the id; the client stops waiting for it.
    NotFound,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLoginCompletion {
    pub login_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    /// Optional desktop onboarding entrypoint; validated against the schema.
    pub onboarding_entrypoint: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountUpdate {
    pub auth_mode: Option<AgentAccountAuthMode>,
    pub plan_type: Option<AgentAccountPlanType>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRateLimitWindow {
    pub used_percent: i32,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentCreditsSnapshot {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentSpendControlLimit {
    pub limit: String,
    pub used: String,
    pub remaining_percent: i32,
    pub resets_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRateLimitReachedType {
    RateLimitReached,
    WorkspaceOwnerCreditsDepleted,
    WorkspaceMemberCreditsDepleted,
    WorkspaceOwnerUsageLimitReached,
    WorkspaceMemberUsageLimitReached,
}

impl AgentRateLimitReachedType {
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "rate_limit_reached" => Self::RateLimitReached,
            "workspace_owner_credits_depleted" => Self::WorkspaceOwnerCreditsDepleted,
            "workspace_member_credits_depleted" => Self::WorkspaceMemberCreditsDepleted,
            "workspace_owner_usage_limit_reached" => Self::WorkspaceOwnerUsageLimitReached,
            "workspace_member_usage_limit_reached" => Self::WorkspaceMemberUsageLimitReached,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRateLimitResetType {
    CodexRateLimits,
    Unknown,
}

impl AgentRateLimitResetType {
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "codexRateLimits" => Self::CodexRateLimits,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRateLimitResetCreditStatus {
    Available,
    Redeeming,
    Redeemed,
    Unknown,
}

impl AgentRateLimitResetCreditStatus {
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "available" => Self::Available,
            "redeeming" => Self::Redeeming,
            "redeemed" => Self::Redeemed,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRateLimitResetCredit {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub granted_at: i64,
    pub expires_at: Option<i64>,
    pub reset_type: AgentRateLimitResetType,
    pub status: AgentRateLimitResetCreditStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRateLimitResetCredits {
    pub available_count: i64,
    /// None means only the count is known; an empty list means the backend
    /// returned no available detail rows.
    pub credits: Option<Vec<AgentRateLimitResetCredit>>,
}

/// One quota bucket, keyed by limitId in AgentAccountRateLimitsState.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRateLimitBucket {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub normal_model_slug: Option<String>,
    pub primary: Option<AgentRateLimitWindow>,
    pub secondary: Option<AgentRateLimitWindow>,
    pub credits: Option<AgentCreditsSnapshot>,
    pub individual_limit: Option<AgentSpendControlLimit>,
    pub spend_control_reached: Option<bool>,
    pub plan_type: Option<AgentAccountPlanType>,
    pub rate_limit_reached_type: Option<AgentRateLimitReachedType>,
}

/// Sparse quota patch. None means the field was absent or explicitly null:
/// nullable backend fields report "currently unavailable" and must not clear a
/// confirmed value, so only present non-null values are applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRateLimitPatch {
    pub limit_id: Option<Option<String>>,
    pub limit_name: Option<Option<String>>,
    pub normal_model_slug: Option<Option<String>>,
    pub primary: Option<Option<AgentRateLimitWindow>>,
    pub secondary: Option<Option<AgentRateLimitWindow>>,
    pub credits: Option<Option<AgentCreditsSnapshot>>,
    pub individual_limit: Option<Option<AgentSpendControlLimit>>,
    pub spend_control_reached: Option<Option<bool>>,
    pub plan_type: Option<Option<AgentAccountPlanType>>,
    pub rate_limit_reached_type: Option<Option<AgentRateLimitReachedType>>,
}

impl AgentRateLimitPatch {
    /// Bucket key this patch belongs to. A patch that omits limitId is the
    /// historical single-bucket format.
    pub fn key(&self) -> &str {
        match self.limit_id {
            Some(Some(ref id)) => id.as_str(),
            _ => AGENT_DEFAULT_RATE_LIMIT_ID,
        }
    }

    /// True when the patch carries at least one confirmed value.
    fn carries_values(&self) -> bool {
        self.limit_id.as_ref().is_some_and(Option::is_some)
            || self.limit_name.as_ref().is_some_and(Option::is_some)
            || self.normal_model_slug.as_ref().is_some_and(Option::is_some)
            || self.primary.as_ref().is_some_and(Option::is_some)
            || self.secondary.as_ref().is_some_and(Option::is_some)
            || self.credits.as_ref().is_some_and(Option::is_some)
            || self.individual_limit.as_ref().is_some_and(Option::is_some)
            || self
                .spend_control_reached
                .as_ref()
                .is_some_and(Option::is_some)
            || self.plan_type.as_ref().is_some_and(Option::is_some)
            || self
                .rate_limit_reached_type
                .as_ref()
                .is_some_and(Option::is_some)
    }

    fn apply_to(&self, bucket: &mut AgentRateLimitBucket) -> bool {
        let mut changed = false;
        macro_rules! apply {
            ($field:ident) => {
                if let Some(Some(value)) = &self.$field {
                    if bucket.$field.as_ref() != Some(value) {
                        bucket.$field = Some(value.clone());
                        changed = true;
                    }
                }
            };
        }
        apply!(limit_id);
        apply!(limit_name);
        apply!(normal_model_slug);
        apply!(individual_limit);
        apply!(spend_control_reached);
        apply!(plan_type);
        apply!(rate_limit_reached_type);
        if let Some(Some(update)) = &self.primary
            && merge_window(&mut bucket.primary, update)
        {
            changed = true;
        }
        if let Some(Some(update)) = &self.secondary
            && merge_window(&mut bucket.secondary, update)
        {
            changed = true;
        }
        if let Some(Some(update)) = &self.credits
            && merge_credits(&mut bucket.credits, update)
        {
            changed = true;
        }
        changed
    }
}

fn merge_window(current: &mut Option<AgentRateLimitWindow>, update: &AgentRateLimitWindow) -> bool {
    match current {
        Some(current) => {
            let mut changed = false;
            if current.used_percent != update.used_percent {
                current.used_percent = update.used_percent;
                changed = true;
            }
            if update.window_duration_mins.is_some()
                && current.window_duration_mins != update.window_duration_mins
            {
                current.window_duration_mins = update.window_duration_mins;
                changed = true;
            }
            if update.resets_at.is_some() && current.resets_at != update.resets_at {
                current.resets_at = update.resets_at;
                changed = true;
            }
            changed
        }
        None => {
            *current = Some(update.clone());
            true
        }
    }
}

fn merge_credits(
    current: &mut Option<AgentCreditsSnapshot>,
    update: &AgentCreditsSnapshot,
) -> bool {
    match current {
        Some(current) => {
            let mut changed = false;
            if current.has_credits != update.has_credits {
                current.has_credits = update.has_credits;
                changed = true;
            }
            if current.unlimited != update.unlimited {
                current.unlimited = update.unlimited;
                changed = true;
            }
            if update.balance.is_some() && current.balance != update.balance {
                current.balance = update.balance.clone();
                changed = true;
            }
            changed
        }
        None => {
            *current = Some(update.clone());
            true
        }
    }
}

/// Complete quota snapshot for the account connected to one generation.
/// Buckets are isolated by accountId + limitId.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountRateLimitsState {
    pub account_id: Option<String>,
    pub ordinary_usage_allowed: Option<bool>,
    pub reset_credits: Option<AgentRateLimitResetCredits>,
    /// Backend-owned upsell banner; nested keys keep the backend contract.
    pub upsell: Option<serde_json::Value>,
    pub buckets: BTreeMap<String, AgentRateLimitBucket>,
}

impl AgentAccountRateLimitsState {
    pub fn bucket(&self, key: &str) -> Option<&AgentRateLimitBucket> {
        self.buckets.get(key)
    }

    /// The legacy single-bucket view the account menu reads for its remaining
    /// percentage, in the same precedence the backend uses.
    pub fn preferred_bucket(&self) -> Option<&AgentRateLimitBucket> {
        self.buckets
            .get(AGENT_DEFAULT_RATE_LIMIT_ID)
            .or_else(|| self.buckets.values().next())
    }

    fn forget_other_accounts(&mut self, account_id: Option<&str>) -> bool {
        if self.account_id.as_deref() == account_id {
            return false;
        }
        let changed = self.account_id.is_some() || !self.buckets.is_empty();
        self.account_id = account_id.map(str::to_owned);
        self.buckets.clear();
        self.reset_credits = None;
        self.upsell = None;
        self.ordinary_usage_allowed = None;
        changed
    }

    fn clear(&mut self) -> bool {
        let changed = self.account_id.is_some()
            || !self.buckets.is_empty()
            || self.reset_credits.is_some()
            || self.upsell.is_some()
            || self.ordinary_usage_allowed.is_some();
        *self = Self::default();
        changed
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRateLimitsRead {
    pub account_id: Option<String>,
    pub ordinary_usage_allowed: Option<bool>,
    pub reset_credits: Option<AgentRateLimitResetCredits>,
    pub upsell: Option<serde_json::Value>,
    /// Buckets in wire order; the legacy view and the multi-bucket view are
    /// merged into one map keyed by limitId.
    pub patches: Vec<AgentRateLimitPatch>,
}

/// Result of a logout round trip. The logout answer alone never counts as the
/// server's final state, so the confirming read is reported separately.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentLogoutOutcome {
    /// Account answer read after the logout response, when that read succeeded.
    pub account: Option<AgentAccountSnapshot>,
    /// Set when the account or quota state could not be confirmed afterwards.
    pub confirmation_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountState {
    /// Connection generation this snapshot belongs to.
    pub generation: u64,
    pub account: AgentAccountSnapshot,
    pub login: AgentAccountLoginState,
    pub rate_limits: AgentAccountRateLimitsState,
}

impl AgentAccountState {
    /// account/read answers the account question authoritatively, including
    /// account = null and a missing account field.
    pub fn apply_account_read(&mut self, snapshot: AgentAccountSnapshot) -> bool {
        if self.account == snapshot {
            return false;
        }
        self.account = snapshot;
        true
    }

    /// account/updated is sparse: nullable fields report unavailability and
    /// keep the previously confirmed value.
    pub fn apply_account_update(&mut self, update: AgentAccountUpdate) -> bool {
        let mut changed = false;
        if update.auth_mode.is_some() && self.account.auth_mode != update.auth_mode {
            self.account.auth_mode = update.auth_mode;
            changed = true;
        }
        if update.plan_type.is_some() && self.account.plan_type != update.plan_type {
            self.account.plan_type = update.plan_type;
            changed = true;
        }
        changed
    }

    /// A read replaces the account association and merges every bucket it
    /// carries. Reading a different account drops the previous account's
    /// buckets instead of mixing two accounts in one snapshot.
    pub fn apply_rate_limits_read(&mut self, read: AgentRateLimitsRead) -> bool {
        let mut changed = self
            .rate_limits
            .forget_other_accounts(read.account_id.as_deref());
        if read.ordinary_usage_allowed.is_some()
            && self.rate_limits.ordinary_usage_allowed != read.ordinary_usage_allowed
        {
            self.rate_limits.ordinary_usage_allowed = read.ordinary_usage_allowed;
            changed = true;
        }
        if let Some(reset_credits) = read.reset_credits {
            changed |= merge_reset_credits(&mut self.rate_limits.reset_credits, reset_credits);
        }
        if read.upsell.is_some() && self.rate_limits.upsell != read.upsell {
            self.rate_limits.upsell = read.upsell;
            changed = true;
        }
        for patch in &read.patches {
            changed |= self.apply_rate_limit_patch(patch);
        }
        changed
    }

    /// account/rateLimits/updated carries one bucket; merging it never touches
    /// another bucket.
    pub fn apply_rate_limit_patch(&mut self, patch: &AgentRateLimitPatch) -> bool {
        let key = patch.key().to_owned();
        if let Some(bucket) = self.rate_limits.buckets.get_mut(&key) {
            return patch.apply_to(bucket);
        }
        // A snapshot without a single confirmed value must not create an empty
        // bucket: an account with no reported quota would otherwise look like a
        // known bucket of unknowns.
        if !patch.carries_values() {
            return false;
        }
        let mut bucket = AgentRateLimitBucket::default();
        let changed = patch.apply_to(&mut bucket);
        if changed {
            self.rate_limits.buckets.insert(key, bucket);
        }
        changed
    }

    /// Records the request the UI is waiting on. The loginId is the only key
    /// later completion, cancellation, and refresh steps may use.
    pub fn apply_login_started(&mut self, start: &AgentLoginStart) -> bool {
        let state = AgentAccountLoginState {
            phase: AgentAccountLoginPhase::InProgress,
            login_id: Some(start.login_id.clone()),
            challenge: Some(start.challenge.clone()),
            error: None,
        };
        if self.login == state {
            return false;
        }
        self.login = state;
        true
    }

    pub fn apply_login_failed(&mut self, login_id: Option<String>, error: String) -> bool {
        let state = AgentAccountLoginState {
            phase: AgentAccountLoginPhase::Failed,
            login_id: login_id.or_else(|| self.login.login_id.clone()),
            challenge: None,
            error: Some(error),
        };
        if self.login == state {
            return false;
        }
        self.login = state;
        true
    }

    /// Correlates a completion with the login this client is waiting on.
    ///
    /// * A completion whose loginId names another operation is ignored.
    /// * A nullable loginId can only be attributed while exactly one login is
    ///   in flight, so a late completion after a cancel cannot revive it.
    /// * Repeating a completion that was already applied changes nothing.
    pub fn apply_login_completed(&mut self, completion: AgentLoginCompletion) -> bool {
        if self.login.phase != AgentAccountLoginPhase::InProgress {
            return false;
        }
        if let Some(login_id) = &completion.login_id
            && self.login.login_id.as_deref() != Some(login_id.as_str())
        {
            return false;
        }
        if completion.success {
            self.login = AgentAccountLoginState {
                phase: AgentAccountLoginPhase::SignedIn,
                login_id: self.login.login_id.clone(),
                challenge: None,
                error: None,
            };
            return true;
        }
        self.login = AgentAccountLoginState {
            phase: AgentAccountLoginPhase::Failed,
            login_id: self.login.login_id.clone(),
            challenge: None,
            error: Some(
                completion
                    .error
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "登录失败，请重试".to_owned()),
            ),
        };
        true
    }

    /// Cancelling only ends the matching in-flight login. An unknown id leaves
    /// a completed or failed login untouched.
    pub fn apply_login_canceled(&mut self, login_id: &str) -> bool {
        if self.login.login_id.as_deref() != Some(login_id) {
            return false;
        }
        if self.login.phase != AgentAccountLoginPhase::InProgress {
            return false;
        }
        self.login = AgentAccountLoginState {
            phase: AgentAccountLoginPhase::Canceled,
            login_id: Some(login_id.to_owned()),
            challenge: None,
            error: None,
        };
        true
    }

    /// account/logout clears account, login, and quota state. The next
    /// account/read is what confirms the server's post-logout answer.
    pub fn clear_for_logout(&mut self) -> bool {
        let changed = self.account != AgentAccountSnapshot::default()
            || self.login != AgentAccountLoginState::default()
            || self.rate_limits.clear();
        self.account = AgentAccountSnapshot::default();
        self.login = AgentAccountLoginState::default();
        self.rate_limits = AgentAccountRateLimitsState::default();
        changed
    }
}

fn merge_reset_credits(
    current: &mut Option<AgentRateLimitResetCredits>,
    update: AgentRateLimitResetCredits,
) -> bool {
    match current {
        Some(current) => {
            let mut changed = false;
            if current.available_count != update.available_count {
                current.available_count = update.available_count;
                changed = true;
            }
            if update.credits.is_some() && current.credits != update.credits {
                current.credits = update.credits;
                changed = true;
            }
            changed
        }
        None => {
            *current = Some(update);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chatgpt_snapshot(email: Option<&str>) -> AgentAccountSnapshot {
        AgentAccountSnapshot {
            requires_openai_auth: true,
            account: AgentAccountPresence::Account(AgentAccount::Chatgpt {
                email: email.map(str::to_owned),
                plan_type: AgentAccountPlanType::Pro,
            }),
            auth_mode: Some(AgentAccountAuthMode::Chatgpt),
            plan_type: Some(AgentAccountPlanType::Pro),
        }
    }

    fn window(used_percent: i32) -> AgentRateLimitWindow {
        AgentRateLimitWindow {
            used_percent,
            window_duration_mins: Some(10_080),
            resets_at: Some(1_788_752_152),
        }
    }

    fn patch(limit_id: Option<&str>, used_percent: i32) -> AgentRateLimitPatch {
        AgentRateLimitPatch {
            limit_id: limit_id.map(|id| Some(id.to_owned())),
            primary: Some(Some(window(used_percent))),
            ..AgentRateLimitPatch::default()
        }
    }

    #[test]
    fn account_read_keeps_missing_null_and_known_accounts_apart() {
        let mut state = AgentAccountState::default();
        // A first answer equal to the initial state reports no change, and the
        // distinction it carries is still observable.
        assert!(!state.apply_account_read(AgentAccountSnapshot {
            requires_openai_auth: false,
            account: AgentAccountPresence::Missing,
            auth_mode: None,
            plan_type: None,
        }));
        assert_eq!(state.account.account, AgentAccountPresence::Missing);
        assert!(state.apply_account_read(chatgpt_snapshot(Some("rita@example.com"))));
        assert_eq!(
            state.account.account,
            AgentAccountPresence::Account(AgentAccount::Chatgpt {
                email: Some("rita@example.com".into()),
                plan_type: AgentAccountPlanType::Pro,
            })
        );
        // A later answer that reports no account is a real change.
        assert!(state.apply_account_read(AgentAccountSnapshot {
            requires_openai_auth: false,
            account: AgentAccountPresence::Missing,
            auth_mode: None,
            plan_type: None,
        }));
        assert_eq!(state.account.account, AgentAccountPresence::Missing);
        assert!(state.apply_account_read(AgentAccountSnapshot {
            requires_openai_auth: true,
            account: AgentAccountPresence::Null,
            auth_mode: None,
            plan_type: None,
        }));
        assert_eq!(state.account.account, AgentAccountPresence::Null);
        assert!(state.apply_account_read(chatgpt_snapshot(Some("rita@example.com"))));
        assert_eq!(state.account.email(), Some("rita@example.com"));
        // A repeated answer changes nothing.
        assert!(!state.apply_account_read(chatgpt_snapshot(Some("rita@example.com"))));
    }

    #[test]
    fn account_update_never_clears_a_confirmed_value() {
        let mut state = AgentAccountState::default();
        state.apply_account_read(chatgpt_snapshot(None));
        // Nullable fields report unavailability: the confirmed plan stays.
        assert!(!state.apply_account_update(AgentAccountUpdate {
            auth_mode: None,
            plan_type: None,
        }));
        assert_eq!(
            state.account.effective_plan_type(),
            Some(AgentAccountPlanType::Pro)
        );
        assert!(state.apply_account_update(AgentAccountUpdate {
            auth_mode: Some(AgentAccountAuthMode::Chatgpt),
            plan_type: Some(AgentAccountPlanType::Team),
        }));
        assert_eq!(state.account.plan_type, Some(AgentAccountPlanType::Team));
    }

    #[test]
    fn policy_defaults_without_a_429_bucket_read() {
        let mut state = AgentAccountState::default();
        let read = AgentRateLimitsRead {
            account_id: Some("acct_1".into()),
            ordinary_usage_allowed: Some(true),
            reset_credits: None,
            upsell: None,
            patches: vec![patch(None, 20)],
        };
        assert!(state.apply_rate_limits_read(read));
        let bucket = state
            .rate_limits
            .bucket(AGENT_DEFAULT_RATE_LIMIT_ID)
            .expect("legacy bucket");
        assert_eq!(bucket.primary.as_ref().unwrap().used_percent, 20);
        assert_eq!(state.rate_limits.account_id.as_deref(), Some("acct_1"));
    }

    #[test]
    fn sparse_updates_merge_only_their_own_bucket() {
        let mut state = AgentAccountState::default();
        state.apply_rate_limits_read(AgentRateLimitsRead {
            account_id: Some("acct_1".into()),
            ordinary_usage_allowed: None,
            reset_credits: None,
            upsell: None,
            patches: vec![patch(Some("codex"), 15), patch(Some("gpt-5-spark"), 40)],
        });
        // A sparse update for the second bucket keeps the first untouched.
        assert!(state.apply_rate_limit_patch(&AgentRateLimitPatch {
            limit_id: Some(Some("gpt-5-spark".into())),
            primary: Some(Some(AgentRateLimitWindow {
                used_percent: 55,
                window_duration_mins: None,
                resets_at: None,
            })),
            credits: Some(Some(AgentCreditsSnapshot {
                has_credits: true,
                unlimited: false,
                balance: None,
            })),
            ..AgentRateLimitPatch::default()
        }));
        let codex = state.rate_limits.bucket("codex").expect("codex bucket");
        assert_eq!(codex.primary.as_ref().unwrap().used_percent, 15);
        assert_eq!(
            codex.primary.as_ref().unwrap().window_duration_mins,
            Some(10_080)
        );
        assert!(codex.credits.is_none());
        let spark = state
            .rate_limits
            .bucket("gpt-5-spark")
            .expect("spark bucket");
        assert_eq!(spark.primary.as_ref().unwrap().used_percent, 55);
        assert_eq!(
            spark.primary.as_ref().unwrap().resets_at,
            Some(1_788_752_152)
        );
        assert_eq!(
            spark
                .credits
                .as_ref()
                .and_then(|credits| credits.balance.clone()),
            None
        );
    }

    #[test]
    fn null_and_absent_fields_do_not_clear_confirmed_quota_values() {
        let mut state = AgentAccountState::default();
        state.apply_rate_limit_patch(&AgentRateLimitPatch {
            limit_id: Some(Some("codex".into())),
            limit_name: Some(Some("Codex".into())),
            plan_type: Some(Some(AgentAccountPlanType::Pro)),
            primary: Some(Some(window(15))),
            ..AgentRateLimitPatch::default()
        });
        // Nothing changed: null fields report unavailability instead of
        // clearing the confirmed name, plan, and window.
        assert!(!state.apply_rate_limit_patch(&AgentRateLimitPatch {
            limit_id: Some(Some("codex".into())),
            limit_name: Some(None),
            plan_type: Some(None),
            primary: Some(None),
            ..AgentRateLimitPatch::default()
        }));
        let bucket = state.rate_limits.bucket("codex").expect("bucket");
        assert_eq!(bucket.limit_name.as_deref(), Some("Codex"));
        assert_eq!(bucket.plan_type, Some(AgentAccountPlanType::Pro));
        assert_eq!(bucket.primary.as_ref().unwrap().used_percent, 15);
    }

    #[test]
    fn reading_another_account_drops_the_previous_snapshot() {
        let mut state = AgentAccountState::default();
        state.apply_rate_limits_read(AgentRateLimitsRead {
            account_id: Some("acct_1".into()),
            ordinary_usage_allowed: Some(true),
            reset_credits: None,
            upsell: None,
            patches: vec![patch(Some("codex"), 15)],
        });
        state.apply_rate_limits_read(AgentRateLimitsRead {
            account_id: Some("acct_2".into()),
            ordinary_usage_allowed: None,
            reset_credits: None,
            upsell: None,
            patches: vec![patch(Some("spark"), 5)],
        });
        assert_eq!(state.rate_limits.account_id.as_deref(), Some("acct_2"));
        assert!(state.rate_limits.bucket("codex").is_none());
        assert!(state.rate_limits.bucket("spark").is_some());
        assert!(state.rate_limits.ordinary_usage_allowed.is_none());
    }

    #[test]
    fn logout_clears_account_login_and_quotas() {
        let mut state = AgentAccountState::default();
        state.apply_account_read(chatgpt_snapshot(Some("rita@example.com")));
        state.apply_rate_limit_patch(&patch(Some("codex"), 15));
        assert!(state.clear_for_logout());
        assert_eq!(state, AgentAccountState::default());
        assert!(!state.clear_for_logout());
    }

    #[test]
    fn login_completion_is_correlated_cancelled_and_idempotent() {
        let mut state = AgentAccountState::default();
        let start = AgentLoginStart {
            login_id: "login_1".into(),
            challenge: AgentLoginChallenge::DeviceCode {
                verification_url: "https://example.com/device".into(),
                user_code: "ABCD-1234".into(),
            },
        };
        assert!(state.apply_login_started(&start));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::InProgress);
        assert_eq!(state.login.login_id.as_deref(), Some("login_1"));
        // Another operation's completion is ignored.
        assert!(!state.apply_login_completed(AgentLoginCompletion {
            login_id: Some("login_other".into()),
            success: true,
            error: None,
            onboarding_entrypoint: None,
        }));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::InProgress);
        // A nullable loginId is attributed while this client has one login.
        assert!(state.apply_login_completed(AgentLoginCompletion {
            login_id: None,
            success: true,
            error: None,
            onboarding_entrypoint: Some("life_sciences".into()),
        }));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::SignedIn);
        assert!(state.login.challenge.is_none());
        // Repeating the completion changes nothing.
        assert!(!state.apply_login_completed(AgentLoginCompletion {
            login_id: Some("login_1".into()),
            success: true,
            error: None,
            onboarding_entrypoint: None,
        }));
    }

    #[test]
    fn a_late_completion_cannot_revive_a_cancelled_login() {
        let mut state = AgentAccountState::default();
        state.apply_login_started(&AgentLoginStart {
            login_id: "login_2".into(),
            challenge: AgentLoginChallenge::AuthUrl {
                auth_url: "https://example.com/auth".into(),
            },
        });
        assert!(state.apply_login_canceled("login_2"));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::Canceled);
        assert!(!state.apply_login_completed(AgentLoginCompletion {
            login_id: Some("login_2".into()),
            success: true,
            error: None,
            onboarding_entrypoint: None,
        }));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::Canceled);
        // Cancelling an unknown or already finished login changes nothing.
        assert!(!state.apply_login_canceled("login_other"));
        assert!(!state.apply_login_canceled("login_2"));
    }

    #[test]
    fn login_failure_keeps_the_error_and_allows_retry() {
        let mut state = AgentAccountState::default();
        state.apply_login_started(&AgentLoginStart {
            login_id: "login_3".into(),
            challenge: AgentLoginChallenge::AuthUrl {
                auth_url: "https://example.com/auth".into(),
            },
        });
        assert!(state.apply_login_completed(AgentLoginCompletion {
            login_id: Some("login_3".into()),
            success: false,
            error: Some("授权被拒绝".into()),
            onboarding_entrypoint: None,
        }));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::Failed);
        assert_eq!(state.login.error.as_deref(), Some("授权被拒绝"));
        // A new request replaces the failed one.
        assert!(state.apply_login_started(&AgentLoginStart {
            login_id: "login_4".into(),
            challenge: AgentLoginChallenge::AuthUrl {
                auth_url: "https://example.com/auth".into(),
            },
        }));
        assert_eq!(state.login.phase, AgentAccountLoginPhase::InProgress);
        assert!(state.login.error.is_none());

        let mut request_error = AgentAccountState::default();
        assert!(request_error.apply_login_failed(None, "登录请求失败：连接已关闭".into()));
        assert_eq!(request_error.login.phase, AgentAccountLoginPhase::Failed);
        assert_eq!(
            request_error.login.error.as_deref(),
            Some("登录请求失败：连接已关闭")
        );
    }
}
