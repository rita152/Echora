//! Isolated compile/test entry for the protocol-neutral permission card.
//!
//! Production app-server dispatch adapts typed domain data into this model;
//! this crate additionally compiles the presentation layer independently of
//! the protocol adapter.

#![allow(dead_code)]

#[path = "../src/i18n.rs"]
mod i18n;
#[path = "../src/components/icons.rs"]
mod icons_impl;
#[path = "../src/theme.rs"]
mod theme;

#[path = "../src/components/callback.rs"]
mod callback_impl;

mod components {
    pub mod callback {
        pub use crate::callback_impl::*;
    }
    pub mod icons {
        pub use crate::icons_impl::*;
    }
}

#[path = "../src/components/permissions_approval.rs"]
mod permissions_approval;

#[test]
fn isolated_component_is_linked_without_protocol_types() {
    let model =
        permissions_approval::PermissionApprovalPresentation::network("isolated-link-check", None);
    assert!(model.should_render());
}

#[test]
fn english_approval_keeps_original_paths_and_request_text() {
    use permissions_approval::{
        PermissionApprovalPresentation, PermissionPathAccess, PermissionPathRequest,
    };
    let previous = i18n::language();
    i18n::set_language(i18n::Language::English);
    let model = PermissionApprovalPresentation::combined(
        "english-request",
        vec![
            PermissionPathRequest::new("/tmp/文件", PermissionPathAccess::Read),
            PermissionPathRequest::new("/tmp/项目", PermissionPathAccess::Write),
        ],
        Some("保留原始请求文本".into()),
    );
    assert_eq!(model.title(), "Permissions");
    assert_eq!(
        model.question_text(),
        "Allow ChatGPT to access the internet, read 文件 and edit 项目?"
    );
    i18n::set_language(previous);
}
