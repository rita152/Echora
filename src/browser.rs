//! The in-app browser behind the right panel's Browser tabs: what typed
//! addresses mean, the browser's own history and saved tabs, and the native
//! page that shows a site. Nothing here depends on GPUI; the panel lives in
//! `components::browser`.

pub mod address;
pub mod error_page;
pub mod history;
pub mod scripts;
pub mod session;
pub mod webview;
