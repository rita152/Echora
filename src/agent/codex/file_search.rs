//! Fuzzy file search request encoding and response decoding.
//!
//! Both the one-shot request and the session form are modelled here; the
//! manager decides which shape a connection can use and keeps the session
//! bookkeeping. Field validation is strict so a schema change fails loudly
//! instead of silently dropping results.

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::workspace_protocol::{object_field, response_result, string_field};
use crate::agent::{
    AgentFileMatchType, AgentFileSearchRequest, AgentFileSearchResult,
    AgentFileSearchSessionCompleted, AgentFileSearchSessionUpdate,
};

/// Session id used by one client-owned file search session. The server treats
/// it as opaque, so a generation-scoped counter is enough.
pub(super) fn build_file_search_params(request: &AgentFileSearchRequest) -> Value {
    json!({
        "query": request.query,
        "roots": request.roots,
        "cancellationToken": request.cancellation_token,
    })
}

pub(super) fn build_file_search_session_start(session_id: &str, roots: &[String]) -> Value {
    json!({ "sessionId": session_id, "roots": roots })
}

pub(super) fn build_file_search_session_update(session_id: &str, query: &str) -> Value {
    json!({ "sessionId": session_id, "query": query })
}

pub(super) fn build_file_search_session_stop(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub(super) fn parse_file_search_response(response: &Value) -> Result<Vec<AgentFileSearchResult>> {
    let result = response_result(response, "fuzzyFileSearch")?;
    parse_file_results(result, "fuzzyFileSearch result")
}

/// Recognises the "session not found" failure the server reports for a session
/// this connection never created, which is how the reference client discovers
/// that an app-server build lacks session support.
pub(super) fn is_unknown_file_search_session(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("fuzzy file search session not found")
}

pub(super) fn parse_session_updated(message: &Value) -> Result<AgentFileSearchSessionUpdate> {
    let params = message
        .get("params")
        .context("fuzzyFileSearch/sessionUpdated 通知缺少 params")?;
    let session_id = string_field(params, "sessionId", "fuzzyFileSearch/sessionUpdated")?;
    let query = string_field(params, "query", "fuzzyFileSearch/sessionUpdated")?;
    let files = parse_file_results(params, "fuzzyFileSearch/sessionUpdated")?;
    Ok(AgentFileSearchSessionUpdate {
        session_id,
        query,
        files,
    })
}

pub(super) fn parse_session_completed(message: &Value) -> Result<AgentFileSearchSessionCompleted> {
    let params = message
        .get("params")
        .context("fuzzyFileSearch/sessionCompleted 通知缺少 params")?;
    let session_id = string_field(params, "sessionId", "fuzzyFileSearch/sessionCompleted")?;
    Ok(AgentFileSearchSessionCompleted { session_id })
}

fn parse_file_results(value: &Value, context: &str) -> Result<Vec<AgentFileSearchResult>> {
    let files = object_field(value, "files", context)?
        .as_array()
        .with_context(|| format!("{context}.files 必须是数组"))?;
    files
        .iter()
        .map(|entry| parse_file_result(entry, context))
        .collect()
}

fn parse_file_result(entry: &Value, context: &str) -> Result<AgentFileSearchResult> {
    let file_name = string_field(entry, "file_name", context)?;
    let match_type = match string_field(entry, "match_type", context)?.as_str() {
        "file" => AgentFileMatchType::File,
        "directory" => AgentFileMatchType::Directory,
        other => bail!("{context}.match_type 为未知值 `{other}`"),
    };
    let path = string_field(entry, "path", context)?;
    let root = string_field(entry, "root", context)?;
    let score = object_field(entry, "score", context)?
        .as_u64()
        .filter(|score| *score <= u64::from(u32::MAX))
        .with_context(|| format!("{context}.score 必须是 uint32"))? as u32;
    let indices = match entry.get("indices") {
        None | Some(Value::Null) => None,
        Some(Value::Array(indices)) => {
            let mut parsed = Vec::with_capacity(indices.len());
            for index in indices {
                let value = index
                    .as_u64()
                    .filter(|index| *index <= u64::from(u32::MAX))
                    .with_context(|| format!("{context}.indices 必须是 uint32 数组"))?;
                parsed.push(value as u32);
            }
            Some(parsed)
        }
        Some(_) => bail!("{context}.indices 必须是 uint32 数组或 null"),
    };
    Ok(AgentFileSearchResult {
        file_name,
        match_type,
        path,
        root,
        score,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(files: Value) -> Value {
        json!({ "result": { "files": files } })
    }

    #[test]
    fn one_shot_response_decodes_files_and_directories() {
        let response = result(json!([
            {
                "file_name": "chat_search.rs",
                "match_type": "file",
                "path": "src/components/chat_search.rs",
                "root": "/Volumes/ExternalSSD/GPUI",
                "score": 42,
                "indices": [0, 1, 5]
            },
            {
                "file_name": "src",
                "match_type": "directory",
                "path": "src",
                "root": "/Volumes/ExternalSSD/GPUI",
                "score": 7
            }
        ]));
        let files = parse_file_search_response(&response).expect("response parses");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].match_type, AgentFileMatchType::File);
        assert_eq!(files[0].score, 42);
        assert_eq!(files[0].indices, Some(vec![0, 1, 5]));
        assert_eq!(files[1].match_type, AgentFileMatchType::Directory);
        assert_eq!(files[1].indices, None);
    }

    #[test]
    fn unknown_match_type_is_rejected() {
        let response = result(json!([{
            "file_name": "a",
            "match_type": "symlink",
            "path": "a",
            "root": "/tmp",
            "score": 1
        }]));
        let error = parse_file_search_response(&response).expect_err("must reject");
        assert!(format!("{error:#}").contains("match_type"));
    }

    #[test]
    fn session_notifications_require_their_fields() {
        let updated = json!({
            "method": "fuzzyFileSearch/sessionUpdated",
            "params": {
                "sessionId": "s-1",
                "query": "chat",
                "files": [{
                    "file_name": "chat.rs",
                    "match_type": "file",
                    "path": "src/chat.rs",
                    "root": "/tmp",
                    "score": 3
                }]
            }
        });
        let parsed = parse_session_updated(&updated).expect("parses");
        assert_eq!(parsed.session_id, "s-1");
        assert_eq!(parsed.query, "chat");
        assert_eq!(parsed.files.len(), 1);

        let completed = json!({
            "method": "fuzzyFileSearch/sessionCompleted",
            "params": { "sessionId": "s-1" }
        });
        assert_eq!(
            parse_session_completed(&completed)
                .expect("parses")
                .session_id,
            "s-1"
        );

        let missing = json!({
            "method": "fuzzyFileSearch/sessionCompleted",
            "params": {}
        });
        assert!(parse_session_completed(&missing).is_err());
    }

    #[test]
    fn missing_session_is_detected_from_the_error_message() {
        assert!(is_unknown_file_search_session(
            "Codex JSON-RPC \"fuzzyFileSearch/sessionUpdate\" 请求 7 失败：{\"message\":\"fuzzy file search session not found: s-1\"}"
        ));
        assert!(!is_unknown_file_search_session("roots must not be empty"));
    }

    #[test]
    fn request_encoding_matches_the_schema() {
        let request = AgentFileSearchRequest {
            query: "editor".to_owned(),
            roots: vec!["/tmp/p0-scratch".to_owned()],
            cancellation_token: Some("cycle-1".to_owned()),
        };
        assert_eq!(
            build_file_search_params(&request),
            json!({
                "query": "editor",
                "roots": ["/tmp/p0-scratch"],
                "cancellationToken": "cycle-1"
            })
        );
        assert_eq!(
            build_file_search_session_start("s-1", &["/tmp/p0-scratch".to_owned()]),
            json!({ "sessionId": "s-1", "roots": ["/tmp/p0-scratch"] })
        );
        assert_eq!(
            build_file_search_session_update("s-1", "editor"),
            json!({ "sessionId": "s-1", "query": "editor" })
        );
        assert_eq!(
            build_file_search_session_stop("s-1"),
            json!({ "sessionId": "s-1" })
        );
    }
}
