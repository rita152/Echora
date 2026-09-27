//! Chats the ChatGPT desktop app already started for a pull request.
//!
//! The reference turns its header `Chat` button into `Open chat` once a chat
//! exists for the pull request, remembering the pair in the persisted atom
//! `pull-request-chat-associations-v2` of `$CODEX_HOME/.codex-global-state.json`.
//! Both apps share the same app-server threads, so the association is read
//! (never written) here to offer the same button.

use std::path::PathBuf;

use serde_json::Value;

const ASSOCIATIONS_KEY: &str = "pull-request-chat-associations-v2";

fn global_state_path() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))?;
    Some(home.join(".codex-global-state.json"))
}

/// The thread the reference associated with `repository` (`owner/name`) and
/// `number`, if any.
pub fn chat_thread(repository: &str, number: u64) -> Option<String> {
    let bytes = std::fs::read(global_state_path()?).ok()?;
    let state: Value = serde_json::from_slice(&bytes).ok()?;
    find_thread(&state, repository, number)
}

fn find_thread(state: &Value, repository: &str, number: u64) -> Option<String> {
    let (owner, name) = repository.split_once('/')?;
    let associations = state
        .get("electron-persisted-atom-state")?
        .get(ASSOCIATIONS_KEY)?
        .as_object()?;
    associations.iter().find_map(|(key, value)| {
        // Keys are `[host?, account?, hostname, login, "[hostname,owner,repo,number]"]`.
        let key: Vec<Value> = serde_json::from_str(key).ok()?;
        let identity: Vec<Value> = serde_json::from_str(key.last()?.as_str()?).ok()?;
        let matches = identity.get(1)?.as_str()?.eq_ignore_ascii_case(owner)
            && identity.get(2)?.as_str()?.eq_ignore_ascii_case(name)
            && identity.get(3)?.as_u64()? == number;
        matches
            .then(|| value.get("threadId")?.as_str().map(str::to_string))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associations_match_owner_repository_and_number() {
        let state = serde_json::json!({
            "electron-persisted-atom-state": {
                ASSOCIATIONS_KEY: {
                    "[null,null,\"github.com\",\"rita152\",\"[\\\"github.com\\\",\\\"rita152\\\",\\\"echora\\\",5]\"]": {
                        "hostId": "local",
                        "threadId": "thread-5"
                    }
                }
            }
        });
        assert_eq!(
            find_thread(&state, "rita152/Echora", 5).as_deref(),
            Some("thread-5")
        );
        assert_eq!(find_thread(&state, "rita152/Echora", 4), None);
        assert_eq!(find_thread(&state, "other/Echora", 5), None);
    }
}
