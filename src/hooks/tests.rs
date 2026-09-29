//! Grouping follows the reference's hooks-settings model; the entries mirror
//! the baseline recording (artifacts/batch2-baseline-*/hooks.wire.json).

use super::*;
use crate::agent::{AgentHookHandler, AgentHookSource, AgentHookTrustStatus};

fn hook(key: &str, event: AgentHookEventName, source: AgentHookSource, order: i64) -> AgentHook {
    AgentHook {
        key: key.into(),
        event_name: event,
        handler: AgentHookHandler::Command {
            command: format!("echo {key}"),
            is_async: false,
        },
        matcher: None,
        timeout_sec: 600,
        status_message: None,
        source,
        source_path: "/probe/home/config.toml".into(),
        plugin_id: None,
        display_order: order,
        enabled: true,
        is_managed: false,
        current_hash: format!("sha256:{key}"),
        trust_status: AgentHookTrustStatus::Untrusted,
        additional_context_limit: None,
    }
}

fn entries() -> Vec<AgentHookListEntry> {
    let mut trusted = hook(
        "user-pre",
        AgentHookEventName::PreToolUse,
        AgentHookSource::User,
        0,
    );
    trusted.trust_status = AgentHookTrustStatus::Trusted;
    let mut modified = hook(
        "user-stop",
        AgentHookEventName::Stop,
        AgentHookSource::User,
        3,
    );
    modified.trust_status = AgentHookTrustStatus::Modified;
    let mut managed = hook(
        "admin",
        AgentHookEventName::Interrupt,
        AgentHookSource::System,
        5,
    );
    managed.is_managed = true;
    managed.trust_status = AgentHookTrustStatus::Managed;
    let mut plugin = hook(
        "plugin",
        AgentHookEventName::SessionStart,
        AgentHookSource::Plugin,
        6,
    );
    plugin.plugin_id = Some("audit@market".into());
    let user = [
        trusted,
        hook(
            "user-post",
            AgentHookEventName::PostToolUse,
            AgentHookSource::User,
            1,
        ),
        modified,
        hook(
            "user-pre-2",
            AgentHookEventName::PreToolUse,
            AgentHookSource::User,
            2,
        ),
    ];
    vec![
        AgentHookListEntry {
            cwd: "/probe/project".into(),
            hooks: user
                .iter()
                .cloned()
                .chain([
                    hook(
                        "project",
                        AgentHookEventName::SessionStart,
                        AgentHookSource::Project,
                        4,
                    ),
                    managed,
                    plugin,
                ])
                .collect(),
            warnings: vec!["invalid matcher".into()],
            errors: vec![],
        },
        // The same user hooks come back for another cwd; they merge by key.
        AgentHookListEntry {
            cwd: "/probe/other".into(),
            hooks: user.to_vec(),
            warnings: vec![],
            errors: vec![AgentHookLoadError {
                path: "/x".into(),
                message: "bad".into(),
            }],
        },
        AgentHookListEntry {
            cwd: "/probe/broken".into(),
            hooks: vec![],
            warnings: vec!["failed to parse hooks config".into()],
            errors: vec![],
        },
    ]
}

#[test]
fn sources_group_in_the_reference_order_with_merged_issues() {
    let groups = group_sources(&entries());
    let selections = |sources: &[HookSource]| {
        sources
            .iter()
            .map(|source| source.selection.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        selections(&groups.config),
        [
            HookSourceSelection::Shared(AgentHookSourceGroup::User),
            HookSourceSelection::Shared(AgentHookSourceGroup::Admin),
        ]
    );
    let user = &groups.config[0];
    assert_eq!(user.hooks.len(), 4, "listed twice, shown once");
    assert_eq!(user.warnings, ["invalid matcher"]);
    assert_eq!(user.errors.len(), 1);
    assert_eq!(user.issue_count(), 2);
    assert_eq!(user.needs_review(), 3);
    assert_eq!(user.trustable().len(), 3);
    // The managed hook is never offered for trust.
    assert!(groups.config[1].trustable().is_empty());
    assert_eq!(
        selections(&groups.projects),
        [HookSourceSelection::Project("/probe/project".into())]
    );
    assert_eq!(groups.projects[0].hooks.len(), 1);
    assert_eq!(
        selections(&groups.plugins),
        [HookSourceSelection::Plugin(Some("audit@market".into()))]
    );
    // An entry with issues and no hooks is an unknown source.
    assert_eq!(
        selections(&groups.other),
        [HookSourceSelection::Shared(AgentHookSourceGroup::Unknown)]
    );
    assert_eq!(groups.other[0].issue_count(), 1);
    let events = user.events();
    assert_eq!(
        events
            .iter()
            .map(|(event, hooks)| (*event, hooks.len()))
            .collect::<Vec<_>>(),
        [
            (AgentHookEventName::PreToolUse, 2),
            (AgentHookEventName::PostToolUse, 1),
            (AgentHookEventName::Stop, 1),
        ]
    );
    assert!(group_sources(&[]).is_empty());
}

#[test]
fn list_cwds_put_the_selected_project_first() {
    assert_eq!(
        list_cwds(
            &["/work/b".into()],
            [
                "/work/c".into(),
                "/work/a".into(),
                "/work/b".into(),
                "/work/a".into()
            ]
        ),
        [PathBuf::from("/work/b"), "/work/a".into(), "/work/c".into()]
    );
}

#[test]
fn only_the_newest_read_applies_and_a_write_is_exclusive() {
    let mut directory = HooksDirectory::default();
    let old = directory.begin_refresh(vec!["/probe/project".into()]);
    let new = directory.begin_refresh(vec!["/probe/project".into()]);
    let snapshot = AgentHooksSnapshot {
        generation: 1,
        cwds: vec![],
        entries: entries(),
    };
    assert!(!directory.accept(old, Ok(snapshot.clone())));
    assert!(directory.snapshot.is_none() && directory.loading);
    assert!(directory.accept(new, Ok(snapshot)));
    // A failed reread keeps the last list and reports the error.
    let failed = directory.begin_refresh(vec!["/probe/project".into()]);
    directory.accept(failed, Err("closed".into()));
    assert!(directory.snapshot.is_some() && directory.error.is_some());

    directory.open = Some(HookSourceSelection::Shared(AgentHookSourceGroup::User));
    let hook = directory.open_source().unwrap().hooks[1].clone();
    let change = AgentHookStateChange {
        key: hook.key.clone(),
        enabled: Some(false),
        trusted_hash: None,
    };
    assert!(directory.begin_write(vec![change.clone()]));
    assert!(!directory.displayed_enabled(&hook));
    assert!(!directory.begin_write(vec![change]), "one write at a time");
    directory.finish_write(Some(HookWriteFailure::Overridden));
    assert!(!directory.writing());
    assert!(directory.displayed_enabled(&hook), "the server value again");
    assert_eq!(
        directory.write.as_ref().unwrap().failure,
        Some(HookWriteFailure::Overridden)
    );
}
