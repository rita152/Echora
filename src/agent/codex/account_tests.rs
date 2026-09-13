//! Protocol tests for the account, login, and quota codecs.

use serde_json::json;

use super::account::{
    login_request, parse_account_rate_limits_updated, parse_account_response,
    parse_account_updated, parse_cancel_login_response, parse_login_completed,
    parse_login_response, parse_rate_limits_response,
};
use super::methods::ensure_server_method_is_defined;
use crate::agent::{
    AGENT_DEFAULT_RATE_LIMIT_ID, AgentAccount, AgentAccountAuthMode, AgentAccountPlanType,
    AgentAccountPresence, AgentLoginCancelOutcome, AgentLoginChallenge, AgentRateLimitReachedType,
    AgentRateLimitResetCreditStatus, AgentRateLimitResetType,
};

fn response(result: serde_json::Value) -> serde_json::Value {
    json!({ "id": 1, "result": result })
}

#[test]
fn account_read_keeps_every_variant_and_the_absent_null_distinction() {
    let signed_in = parse_account_response(&response(json!({
        "requiresOpenaiAuth": true,
        "account": {"type": "chatgpt", "email": "rita@example.com", "planType": "pro"}
    })))
    .unwrap();
    assert!(signed_in.requires_openai_auth);
    assert_eq!(
        signed_in.account,
        AgentAccountPresence::Account(AgentAccount::Chatgpt {
            email: Some("rita@example.com".into()),
            plan_type: AgentAccountPlanType::Pro,
        })
    );
    assert_eq!(signed_in.email(), Some("rita@example.com"));

    // A null email stays null instead of becoming an empty name.
    let without_email = parse_account_response(&response(json!({
        "requiresOpenaiAuth": true,
        "account": {"type": "chatgpt", "email": null, "planType": "unknown"}
    })))
    .unwrap();
    assert_eq!(without_email.email(), None);
    assert_eq!(
        without_email.effective_plan_type(),
        Some(AgentAccountPlanType::Unknown)
    );

    let signed_out = parse_account_response(&response(json!({
        "requiresOpenaiAuth": true,
        "account": null
    })))
    .unwrap();
    assert_eq!(signed_out.account, AgentAccountPresence::Null);

    // A missing field is not the same answer as null.
    let absent = parse_account_response(&response(json!({"requiresOpenaiAuth": false}))).unwrap();
    assert_eq!(absent.account, AgentAccountPresence::Missing);

    let api_key = parse_account_response(&response(
        json!({"requiresOpenaiAuth": false, "account": {"type": "apiKey"}}),
    ))
    .unwrap();
    assert_eq!(
        api_key.account,
        AgentAccountPresence::Account(AgentAccount::ApiKey)
    );

    let bedrock = parse_account_response(&response(json!({
        "requiresOpenaiAuth": false,
        "account": {"type": "amazonBedrock", "usesCodexManagedCredentials": true}
    })))
    .unwrap();
    assert_eq!(
        bedrock.account,
        AgentAccountPresence::Account(AgentAccount::AmazonBedrock {
            uses_codex_managed_credentials: true,
        })
    );

    // An unsupported variant, an unknown plan, and a missing result all fail
    // loudly instead of reporting a signed-in account.
    let unknown_variant = parse_account_response(&response(json!({
        "requiresOpenaiAuth": true,
        "account": {"type": "futureAuth"}
    })))
    .unwrap_err()
    .to_string();
    assert!(unknown_variant.contains("account/read"));
    let unknown_plan = parse_account_response(&response(json!({
        "requiresOpenaiAuth": true,
        "account": {"type": "chatgpt", "email": null, "planType": "platinum"}
    })))
    .unwrap_err()
    .to_string();
    assert!(unknown_plan.contains("platinum"));
    assert!(parse_account_response(&json!({"id": 1})).is_err());
}

#[test]
fn rate_limits_read_merges_the_legacy_and_multi_bucket_views() {
    let read = parse_rate_limits_response(&response(json!({
        "accountId": "acct_1",
        "ordinaryUsageAllowed": true,
        "rateLimitResetCredits": {
            "availableCount": 2,
            "credits": [{
                "id": "credit_1",
                "title": "完全重置",
                "description": null,
                "grantedAt": 1788000000,
                "expiresAt": null,
                "resetType": "codexRateLimits",
                "status": "available"
            }]
        },
        "rateLimitUpsell": {"message": "升级以获得更高额度"},
        "rateLimits": {"limitId": "codex", "primary": {"usedPercent": 27, "windowDurationMins": 10080, "resetsAt": 1788752152}},
        "rateLimitsByLimitId": {
            "codex": {"limitId": "codex", "limitName": "Codex", "primary": {"usedPercent": 27, "windowDurationMins": 10080, "resetsAt": 1788752152}},
            "gpt-5-spark": {"limitId": "gpt-5-spark", "limitName": "GPT-5 Spark", "primary": {"usedPercent": 0, "windowDurationMins": 300}, "secondary": null}
        }
    })))
    .unwrap();
    assert_eq!(read.account_id.as_deref(), Some("acct_1"));
    assert_eq!(read.ordinary_usage_allowed, Some(true));
    assert_eq!(read.patches.len(), 3);
    let spark = read
        .patches
        .iter()
        .find(|patch| patch.key() == "gpt-5-spark")
        .expect("spark bucket");
    assert_eq!(spark.secondary, Some(None));
    let credits = read.reset_credits.expect("reset credits");
    assert_eq!(credits.available_count, 2);
    let credit = credits.credits.unwrap().remove(0);
    assert_eq!(credit.title.as_deref(), Some("完全重置"));
    assert_eq!(credit.reset_type, AgentRateLimitResetType::CodexRateLimits);
    assert_eq!(credit.status, AgentRateLimitResetCreditStatus::Available);
    assert_eq!(read.upsell.unwrap()["message"], json!("升级以获得更高额度"));

    // A legacy-only read keeps using the default bucket key.
    let legacy = parse_rate_limits_response(&response(json!({
        "rateLimits": {"primary": {"usedPercent": 15}}
    })))
    .unwrap();
    assert_eq!(legacy.patches.len(), 1);
    assert_eq!(legacy.patches[0].key(), AGENT_DEFAULT_RATE_LIMIT_ID);
    assert!(legacy.account_id.is_none());
    assert!(legacy.reset_credits.is_none());

    // A key that disagrees with the entry's own limitId is a protocol error.
    let mismatched = parse_rate_limits_response(&response(json!({
        "rateLimits": {},
        "rateLimitsByLimitId": {"codex": {"limitId": "spark"}}
    })))
    .unwrap_err()
    .to_string();
    assert!(mismatched.contains("limitId"));

    let unknown_reset_type = parse_rate_limits_response(&response(json!({
        "rateLimits": {},
        "rateLimitResetCredits": {
            "availableCount": 1,
            "credits": [{"id": "c", "grantedAt": 1, "resetType": "future", "status": "available"}]
        }
    })))
    .unwrap_err()
    .to_string();
    assert!(unknown_reset_type.contains("resetType"));
}

#[test]
fn account_notifications_validate_their_nullable_fields() {
    ensure_server_method_is_defined(&json!({
        "method": "account/updated",
        "params": {"authMode": "chatgpt", "planType": "pro"}
    }))
    .unwrap();
    let update = parse_account_updated(&json!({
        "method": "account/updated",
        "params": {"authMode": "chatgpt", "planType": null}
    }))
    .unwrap();
    assert_eq!(update.auth_mode, Some(AgentAccountAuthMode::Chatgpt));
    assert_eq!(update.plan_type, None);
    assert!(
        parse_account_updated(&json!({
            "method": "account/updated",
            "params": {"authMode": "futureMode"}
        }))
        .is_err()
    );
    assert!(parse_account_updated(&json!({"method": "account/updated"})).is_err());

    ensure_server_method_is_defined(&json!({
        "method": "account/login/completed",
        "params": {"success": true, "loginId": "login_1", "error": null}
    }))
    .unwrap();
    let completion = parse_login_completed(&json!({
        "method": "account/login/completed",
        "params": {"success": false, "loginId": null, "error": "授权被拒绝"}
    }))
    .unwrap();
    assert!(!completion.success);
    assert!(completion.login_id.is_none());
    assert_eq!(completion.error.as_deref(), Some("授权被拒绝"));
    assert!(
        parse_login_completed(&json!({
            "method": "account/login/completed",
            "params": {"success": true, "onboardingEntrypoint": "futureEntry"}
        }))
        .is_err()
    );

    let patch = parse_account_rate_limits_updated(&json!({
        "method": "account/rateLimits/updated",
        "params": {"rateLimits": {"limitId": "codex", "rateLimitReachedType": "rate_limit_reached"}}
    }))
    .unwrap();
    assert_eq!(
        patch.rate_limit_reached_type,
        Some(Some(AgentRateLimitReachedType::RateLimitReached))
    );
}

#[test]
fn login_variants_decode_and_credentials_never_reach_error_text() {
    let auth_url = parse_login_response(
        &response(json!({
            "type": "chatgpt",
            "authUrl": "https://example.com/auth",
            "loginId": "login_1"
        })),
        "chatgpt",
    )
    .unwrap();
    assert_eq!(auth_url.login_id, "login_1");
    assert_eq!(
        auth_url.challenge,
        AgentLoginChallenge::AuthUrl {
            auth_url: "https://example.com/auth".into(),
        }
    );

    let device_code = parse_login_response(
        &response(json!({
            "type": "chatgptDeviceCode",
            "loginId": "login_2",
            "userCode": "ABCD-1234",
            "verificationUrl": "https://example.com/device"
        })),
        "chatgpt",
    )
    .unwrap();
    assert_eq!(
        device_code.challenge,
        AgentLoginChallenge::DeviceCode {
            verification_url: "https://example.com/device".into(),
            user_code: "ABCD-1234".into(),
        }
    );

    // Unsupported variants are named, and the secrets they carry are never
    // repeated in the error the UI can display.
    let tokens = parse_login_response(
        &response(json!({
            "type": "chatgptAuthTokens",
            "accessToken": "secret-access-token",
            "chatgptAccountId": "acct"
        })),
        "chatgpt",
    )
    .unwrap_err()
    .to_string();
    assert!(tokens.contains("chatgptAuthTokens"));
    assert!(!tokens.contains("secret-access-token"));
    assert!(
        parse_login_response(&response(json!({"type": "apiKey"})), "chatgpt")
            .unwrap_err()
            .to_string()
            .contains("apiKey")
    );
    assert!(
        parse_login_response(&response(json!({"type": "amazonBedrock"})), "chatgpt")
            .unwrap_err()
            .to_string()
            .contains("amazonBedrock")
    );
    assert!(parse_login_response(&response(json!({"type": "chatgpt"})), "chatgpt").is_err());

    let canceled = parse_cancel_login_response(&response(json!({"status": "canceled"}))).unwrap();
    assert_eq!(canceled, AgentLoginCancelOutcome::Canceled);
    let missing = parse_cancel_login_response(&response(json!({"status": "notFound"}))).unwrap();
    assert_eq!(missing, AgentLoginCancelOutcome::NotFound);
    assert!(parse_cancel_login_response(&response(json!({"status": "pending"}))).is_err());
}

#[test]
fn only_the_codex_managed_chatgpt_login_can_be_requested() {
    assert_eq!(
        login_request("chatgpt").unwrap(),
        json!({"type": "chatgpt"})
    );
    for unsupported in [
        "apiKey",
        "chatgptAuthTokens",
        "amazonBedrock",
        "chatgptDeviceCode",
    ] {
        let error = login_request(unsupported).unwrap_err().to_string();
        assert!(error.contains(unsupported), "{error}");
    }
}
