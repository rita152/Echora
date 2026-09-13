//! Account, login, and quota codecs for the app-server protocol.
//!
//! Request responses return errors to their caller, so an unsupported account
//! variant or a schema violation is reported in the account surfaces without
//! claiming a state the backend never announced. Connection notifications stay
//! strict: an unknown enum value fails loudly instead of being guessed.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::agent::{
    AgentAccount, AgentAccountAuthMode, AgentAccountPlanType, AgentAccountPresence,
    AgentAccountSnapshot, AgentAccountUpdate, AgentCreditsSnapshot, AgentLoginCancelOutcome,
    AgentLoginChallenge, AgentLoginCompletion, AgentLoginStart, AgentRateLimitPatch,
    AgentRateLimitReachedType, AgentRateLimitResetCredit, AgentRateLimitResetCreditStatus,
    AgentRateLimitResetCredits, AgentRateLimitResetType, AgentRateLimitWindow, AgentRateLimitsRead,
    AgentSpendControlLimit,
};

/// Distinguishes an absent field from an explicit null, which the account
/// contract requires for both account/read and the nullable quota metadata.
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AccountPayload {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "chatgpt")]
    Chatgpt {
        email: Option<String>,
        #[serde(rename = "planType")]
        plan_type: String,
    },
    #[serde(rename = "amazonBedrock")]
    AmazonBedrock {
        #[serde(default, rename = "usesCodexManagedCredentials")]
        uses_codex_managed_credentials: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetAccountResponse {
    requires_openai_auth: bool,
    #[serde(default, deserialize_with = "double_option")]
    account: Option<Option<AccountPayload>>,
}

fn plan_type(value: &str, context: &str) -> Result<AgentAccountPlanType> {
    AgentAccountPlanType::from_wire(value)
        .with_context(|| format!("{context} 为未知套餐值 {}", quote(value)))
}

fn quote(value: &str) -> String {
    format!("'{value}'")
}

pub(super) fn parse_account_response(message: &Value) -> Result<AgentAccountSnapshot> {
    let value = message
        .get("result")
        .context("account/read 响应缺少 result")?;
    let response: GetAccountResponse =
        serde_json::from_value(value.clone()).context("account/read 响应不符合协议 schema")?;
    let account = match response.account {
        None => AgentAccountPresence::Missing,
        Some(None) => AgentAccountPresence::Null,
        Some(Some(payload)) => AgentAccountPresence::Account(match payload {
            AccountPayload::ApiKey => AgentAccount::ApiKey,
            AccountPayload::Chatgpt {
                email,
                plan_type: wire,
            } => AgentAccount::Chatgpt {
                email,
                plan_type: plan_type(&wire, "account/read 的 account.planType")?,
            },
            AccountPayload::AmazonBedrock {
                uses_codex_managed_credentials,
            } => AgentAccount::AmazonBedrock {
                uses_codex_managed_credentials,
            },
        }),
    };
    Ok(AgentAccountSnapshot {
        requires_openai_auth: response.requires_openai_auth,
        account,
        auth_mode: None,
        plan_type: None,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitWindowPayload {
    used_percent: i32,
    #[serde(default)]
    window_duration_mins: Option<i64>,
    #[serde(default)]
    resets_at: Option<i64>,
}

impl RateLimitWindowPayload {
    fn into_domain(self) -> AgentRateLimitWindow {
        AgentRateLimitWindow {
            used_percent: self.used_percent,
            window_duration_mins: self.window_duration_mins,
            resets_at: self.resets_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditsPayload {
    has_credits: bool,
    unlimited: bool,
    #[serde(default)]
    balance: Option<String>,
}

impl CreditsPayload {
    fn into_domain(self) -> AgentCreditsSnapshot {
        AgentCreditsSnapshot {
            has_credits: self.has_credits,
            unlimited: self.unlimited,
            balance: self.balance,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpendControlPayload {
    limit: String,
    remaining_percent: i32,
    resets_at: i64,
    used: String,
}

impl SpendControlPayload {
    fn into_domain(self) -> AgentSpendControlLimit {
        AgentSpendControlLimit {
            limit: self.limit,
            used: self.used,
            remaining_percent: self.remaining_percent,
            resets_at: self.resets_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimitSnapshotPayload {
    #[serde(default, deserialize_with = "double_option")]
    limit_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    limit_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    normal_model_slug: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    primary: Option<Option<RateLimitWindowPayload>>,
    #[serde(default, deserialize_with = "double_option")]
    secondary: Option<Option<RateLimitWindowPayload>>,
    #[serde(default, deserialize_with = "double_option")]
    credits: Option<Option<CreditsPayload>>,
    #[serde(default, deserialize_with = "double_option")]
    individual_limit: Option<Option<SpendControlPayload>>,
    #[serde(default, deserialize_with = "double_option")]
    spend_control_reached: Option<Option<bool>>,
    #[serde(default, deserialize_with = "double_option")]
    plan_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    rate_limit_reached_type: Option<Option<String>>,
}

impl RateLimitSnapshotPayload {
    fn into_patch(self, context: &str) -> Result<AgentRateLimitPatch> {
        let plan_type = match &self.plan_type {
            Some(Some(wire)) => Some(Some(plan_type(wire, &format!("{context} 的 planType"))?)),
            Some(None) => Some(None),
            None => None,
        };
        let rate_limit_reached_type = match &self.rate_limit_reached_type {
            Some(Some(wire)) => Some(Some(
                AgentRateLimitReachedType::from_wire(wire).with_context(|| {
                    format!("{context} 的 rateLimitReachedType 为未知值 {}", quote(wire))
                })?,
            )),
            Some(None) => Some(None),
            None => None,
        };
        Ok(AgentRateLimitPatch {
            limit_id: self.limit_id,
            limit_name: self.limit_name,
            normal_model_slug: self.normal_model_slug,
            primary: self
                .primary
                .map(|window| window.map(RateLimitWindowPayload::into_domain)),
            secondary: self
                .secondary
                .map(|window| window.map(RateLimitWindowPayload::into_domain)),
            credits: self
                .credits
                .map(|credits| credits.map(CreditsPayload::into_domain)),
            individual_limit: self
                .individual_limit
                .map(|limit| limit.map(SpendControlPayload::into_domain)),
            spend_control_reached: self.spend_control_reached,
            plan_type,
            rate_limit_reached_type,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetCreditPayload {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    granted_at: i64,
    #[serde(default)]
    expires_at: Option<i64>,
    reset_type: String,
    status: String,
}

impl ResetCreditPayload {
    fn into_domain(self) -> Result<AgentRateLimitResetCredit> {
        let reset_type =
            AgentRateLimitResetType::from_wire(&self.reset_type).with_context(|| {
                format!(
                    "account/rateLimits 的 rateLimitResetCredits.resetType 为未知值 {}",
                    quote(&self.reset_type)
                )
            })?;
        let status =
            AgentRateLimitResetCreditStatus::from_wire(&self.status).with_context(|| {
                format!(
                    "account/rateLimits 的 rateLimitResetCredits.status 为未知值 {}",
                    quote(&self.status)
                )
            })?;
        Ok(AgentRateLimitResetCredit {
            id: self.id,
            title: self.title,
            description: self.description,
            granted_at: self.granted_at,
            expires_at: self.expires_at,
            reset_type,
            status,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetCreditsPayload {
    available_count: i64,
    /// null means only the count is known, while an empty array means the
    /// backend returned no available detail rows.
    #[serde(default, deserialize_with = "double_option")]
    credits: Option<Option<Vec<ResetCreditPayload>>>,
}

impl ResetCreditsPayload {
    fn into_domain(self) -> Result<AgentRateLimitResetCredits> {
        let credits = match self.credits {
            Some(Some(credits)) => Some(
                credits
                    .into_iter()
                    .map(ResetCreditPayload::into_domain)
                    .collect::<Result<Vec<_>>>()?,
            ),
            _ => None,
        };
        Ok(AgentRateLimitResetCredits {
            available_count: self.available_count,
            credits,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetAccountRateLimitsResponse {
    #[serde(default, deserialize_with = "double_option")]
    account_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    ordinary_usage_allowed: Option<Option<bool>>,
    #[serde(default)]
    rate_limit_reset_credits: Option<ResetCreditsPayload>,
    #[serde(default)]
    rate_limit_upsell: Option<Value>,
    rate_limits: RateLimitSnapshotPayload,
    #[serde(default)]
    rate_limits_by_limit_id: Option<BTreeMap<String, RateLimitSnapshotPayload>>,
}

pub(super) fn parse_rate_limits_response(message: &Value) -> Result<AgentRateLimitsRead> {
    let value = message
        .get("result")
        .context("account/rateLimits/read 响应缺少 result")?;
    let response: GetAccountRateLimitsResponse = serde_json::from_value(value.clone())
        .context("account/rateLimits/read 响应不符合协议 schema")?;
    let mut patches = vec![
        response
            .rate_limits
            .into_patch("account/rateLimits/read 的 rateLimits")?,
    ];
    if let Some(by_limit_id) = response.rate_limits_by_limit_id {
        for (limit_id, payload) in by_limit_id {
            if let Some(Some(inner)) = &payload.limit_id
                && inner != &limit_id
            {
                bail!(
                    "account/rateLimits/read 的 rateLimitsByLimitId 键 {} 与条目 limitId {} 不一致",
                    quote(&limit_id),
                    quote(inner)
                );
            }
            patches.push(payload.into_patch(&format!(
                "account/rateLimits/read 的 rateLimitsByLimitId.{limit_id}"
            ))?);
        }
    }
    Ok(AgentRateLimitsRead {
        account_id: response.account_id.and_then(|value| value),
        ordinary_usage_allowed: response.ordinary_usage_allowed.and_then(|value| value),
        reset_credits: match response.rate_limit_reset_credits {
            Some(credits) => Some(credits.into_domain()?),
            None => None,
        },
        upsell: response.rate_limit_upsell,
        patches,
    })
}

pub(super) fn parse_account_rate_limits_updated(message: &Value) -> Result<AgentRateLimitPatch> {
    let params = message
        .get("params")
        .context("account/rateLimits/updated 通知缺少 params")?;
    let payload: RateLimitSnapshotPayload = serde_json::from_value(
        params
            .get("rateLimits")
            .cloned()
            .context("account/rateLimits/updated 通知缺少 params.rateLimits")?,
    )
    .context("account/rateLimits/updated 通知 params.rateLimits 不符合协议 schema")?;
    payload.into_patch("account/rateLimits/updated 的 params.rateLimits")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountUpdatedNotification {
    #[serde(default, deserialize_with = "double_option")]
    auth_mode: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    plan_type: Option<Option<String>>,
}

pub(super) fn parse_account_updated(message: &Value) -> Result<AgentAccountUpdate> {
    let params = message
        .get("params")
        .context("account/updated 通知缺少 params")?;
    let notification: AccountUpdatedNotification = serde_json::from_value(params.clone())
        .context("account/updated 通知 params 不符合协议 schema")?;
    let auth_mode = match notification.auth_mode {
        Some(Some(wire)) => Some(AgentAccountAuthMode::from_wire(&wire).with_context(|| {
            format!("account/updated 通知的 authMode 为未知值 {}", quote(&wire))
        })?),
        _ => None,
    };
    let plan_type = match notification.plan_type {
        Some(Some(wire)) => Some(plan_type(&wire, "account/updated 通知的 planType")?),
        _ => None,
    };
    Ok(AgentAccountUpdate {
        auth_mode,
        plan_type,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountLoginCompletedNotification {
    success: bool,
    #[serde(default)]
    login_id: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    onboarding_entrypoint: Option<String>,
}

pub(super) fn parse_login_completed(message: &Value) -> Result<AgentLoginCompletion> {
    let params = message
        .get("params")
        .context("account/login/completed 通知缺少 params")?;
    let notification: AccountLoginCompletedNotification = serde_json::from_value(params.clone())
        .context("account/login/completed 通知 params 不符合协议 schema")?;
    if let Some(entrypoint) = notification.onboarding_entrypoint.as_deref()
        && entrypoint != "life_sciences"
    {
        bail!(
            "account/login/completed 通知的 onboardingEntrypoint 为未知值 {}",
            quote(entrypoint)
        );
    }
    Ok(AgentLoginCompletion {
        login_id: notification.login_id,
        success: notification.success,
        error: notification.error,
        onboarding_entrypoint: notification.onboarding_entrypoint,
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum LoginAccountResponse {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "chatgpt")]
    Chatgpt {
        #[serde(rename = "authUrl")]
        auth_url: String,
        #[serde(rename = "loginId")]
        login_id: String,
    },
    #[serde(rename = "chatgptDeviceCode")]
    ChatgptDeviceCode {
        #[serde(rename = "loginId")]
        login_id: String,
        #[serde(rename = "userCode")]
        user_code: String,
        #[serde(rename = "verificationUrl")]
        verification_url: String,
    },
    #[serde(rename = "chatgptAuthTokens")]
    ChatgptAuthTokens,
    #[serde(rename = "amazonBedrock")]
    AmazonBedrock,
}

/// Decodes the login response variants this phase supports. Other variants are
/// named in the error instead of being reported as a started login.
pub(super) fn parse_login_response(message: &Value, requested: &str) -> Result<AgentLoginStart> {
    let value = message
        .get("result")
        .context("account/login/start 响应缺少 result")?;
    let response: LoginAccountResponse = serde_json::from_value(value.clone())
        .context("account/login/start 响应不符合协议 schema")?;
    match response {
        LoginAccountResponse::Chatgpt { auth_url, login_id } => Ok(AgentLoginStart {
            login_id,
            challenge: AgentLoginChallenge::AuthUrl { auth_url },
        }),
        LoginAccountResponse::ChatgptDeviceCode {
            login_id,
            user_code,
            verification_url,
        } => Ok(AgentLoginStart {
            login_id,
            challenge: AgentLoginChallenge::DeviceCode {
                verification_url,
                user_code,
            },
        }),
        LoginAccountResponse::ApiKey => bail!(
            "account/login/start 返回了 {requested} 请求之外的 apiKey 变体；本阶段未接入 API key 登录"
        ),
        LoginAccountResponse::ChatgptAuthTokens => bail!(
            "account/login/start 返回了 {requested} 请求之外的 chatgptAuthTokens 变体；外部 token 刷新不在本阶段范围内"
        ),
        LoginAccountResponse::AmazonBedrock => bail!(
            "account/login/start 返回了 {requested} 请求之外的 amazonBedrock 变体；Bedrock 登录不在本阶段范围内"
        ),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelLoginAccountResponse {
    status: String,
}

pub(super) fn parse_cancel_login_response(message: &Value) -> Result<AgentLoginCancelOutcome> {
    let value = message
        .get("result")
        .context("account/login/cancel 响应缺少 result")?;
    let response: CancelLoginAccountResponse = serde_json::from_value(value.clone())
        .context("account/login/cancel 响应不符合协议 schema")?;
    match response.status.as_str() {
        "canceled" => Ok(AgentLoginCancelOutcome::Canceled),
        "notFound" => Ok(AgentLoginCancelOutcome::NotFound),
        other => bail!("account/login/cancel 响应 status 为未知值 {}", quote(other)),
    }
}

/// Login request body. Only the Codex-managed ChatGPT flow is a visible
/// product entry point in this phase.
pub(super) fn login_request(login_type: &str) -> Result<Value> {
    match login_type {
        "chatgpt" => Ok(serde_json::json!({ "type": "chatgpt" })),
        other => bail!("不支持的 account/login/start 登录类型 {}", quote(other)),
    }
}
