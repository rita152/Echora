//! The page a tab shows when a site cannot be loaded or its page crashed.
//!
//! The reference renders Chromium's network-error interstitial as HTML in the
//! tab; its content comes from `WHe` (load errors) and `GHe` (crashes) in the
//! main process: a heading, a summary keyed on the Chromium error name, a
//! "Try:" list with a link that reveals four troubleshooting details, the
//! error code, and Reload (plus "Open in external browser" after a crash).

use super::address;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPage {
    pub heading: String,
    pub summary: String,
    pub try_label: Option<String>,
    pub suggestions: Vec<String>,
    /// The suggestion that toggles `details`.
    pub details_link: Option<String>,
    pub details: Vec<(String, String)>,
    /// Shown uppercase under the summary, e.g. `ERR_NAME_NOT_RESOLVED`.
    pub error_code: Option<String>,
    pub reload: String,
    pub open_external: Option<String>,
    /// The crash page puts its buttons at the end of the row.
    pub actions_at_end: bool,
}

/// The app name the reference's network-access advice names.
const APP_NAME: &str = "Echora";

pub fn load_error(failed_url: &str, error_code: &str) -> ErrorPage {
    let host = host_label(failed_url);
    let code = error_code.trim().to_uppercase();
    let summary = match code.as_str() {
        "DNS_PROBE_POSSIBLE" | "ERR_NAME_NOT_RESOLVED" => crate::i18n::format!(
            "无法找到 {host} 的服务器 IP 地址" => "{host}'s server IP address could not be found"
        ),
        "ERR_INTERNET_DISCONNECTED" => crate::i18n::format!(
            "无法加载 {host}，因为计算机处于离线状态" => "{host} could not be loaded because the computer is offline"
        ),
        "ERR_CONNECTION_REFUSED" => {
            crate::i18n::format!("{host} 拒绝建立连接" => "{host} refused to connect")
        }
        "ERR_CONNECTION_TIMED_OUT" | "ERR_TIMED_OUT" => {
            crate::i18n::format!("{host} 响应超时" => "{host} took too long to respond")
        }
        code if code.starts_with("ERR_CERT_") => crate::i18n::format!(
            "无法验证 {host} 的证书" => "{host}'s certificate could not be verified"
        ),
        _ => crate::i18n::format!("无法加载 {host}" => "{host} could not be loaded"),
    };
    ErrorPage {
        heading: crate::i18n::format!("无法访问此站点" => "This site can't be reached"),
        summary,
        try_label: Some(crate::i18n::format!("尝试：" => "Try:")),
        suggestions: vec![crate::i18n::format!("检查网络连接" => "Checking the connection")],
        details_link: Some(crate::i18n::format!(
            "检查代理、防火墙和 DNS 配置" => "Checking the proxy, firewall, and DNS configuration"
        )),
        details: vec![
            (
                crate::i18n::format!("检查网络连接" => "Check your Internet connection"),
                crate::i18n::format!(
                    "检查所有线缆连接，并重启你当前使用的路由器、调制解调器或其他网络设备" =>
                    "Check any cables and restart any routers, modems, or other network devices you may be using"
                ),
            ),
            (
                crate::i18n::format!("检查 DNS 设置" => "Check your DNS settings"),
                crate::i18n::format!(
                    "如果你不清楚这表示什么，请联系网络管理员" =>
                    "Contact your network administrator if you are not sure what this means"
                ),
            ),
            (
                crate::i18n::format!(
                    "在防火墙或安全设置中允许 {APP_NAME} 访问网络" =>
                    "Allow {APP_NAME} to access the network in your firewall or security settings"
                ),
                crate::i18n::format!(
                    "如果 {APP_NAME} 已在允许的应用列表中，请尝试将其从列表中移除，然后重新添加" =>
                    "If {APP_NAME} is already listed as an allowed app, try removing it from the list and adding it again"
                ),
            ),
            (
                crate::i18n::format!("如果使用代理服务器" => "If you use a proxy server"),
                crate::i18n::format!(
                    "打开系统网络设置，检查当前网络是否配置了代理" =>
                    "Open your system network settings and check whether a proxy has been configured for the active network"
                ),
            ),
        ],
        error_code: Some(if code.is_empty() {
            "ERR_FAILED".to_owned()
        } else {
            code
        }),
        reload: crate::i18n::format!("重新加载" => "Reload"),
        open_external: None,
        actions_at_end: false,
    }
}

pub fn crash(page_url: &str) -> ErrorPage {
    let host = host_label(page_url);
    ErrorPage {
        heading: crate::i18n::format!("此页面已崩溃" => "This page crashed"),
        summary: crate::i18n::format!("{host} 意外崩溃" => "{host} crashed unexpectedly"),
        try_label: None,
        suggestions: Vec::new(),
        details_link: None,
        details: Vec::new(),
        error_code: None,
        reload: crate::i18n::format!("重新加载" => "Reload"),
        open_external: address::is_web_url(page_url)
            .then(|| crate::i18n::format!("在外部浏览器中打开" => "Open in external browser")),
        actions_at_end: true,
    }
}

/// `sU`: the failed URL's host, else its path, else the text itself.
fn host_label(url: &str) -> String {
    address::host(url).unwrap_or_else(|| url.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_errors_pick_the_reference_summary() {
        crate::i18n::set_language(crate::i18n::Language::English);
        let page = load_error("https://nope.invalid/x", "ERR_NAME_NOT_RESOLVED");
        assert_eq!(page.heading, "This site can't be reached");
        assert_eq!(
            page.summary,
            "nope.invalid's server IP address could not be found"
        );
        assert_eq!(page.error_code.as_deref(), Some("ERR_NAME_NOT_RESOLVED"));
        assert_eq!(page.details.len(), 4);
        let refused = load_error("http://localhost:9/", "ERR_CONNECTION_REFUSED");
        assert_eq!(refused.summary, "localhost refused to connect");
        let cert = load_error("https://expired.example/", "ERR_CERT_DATE_INVALID");
        assert_eq!(
            cert.summary,
            "expired.example's certificate could not be verified"
        );
        let crashed = crash("https://example.net/");
        assert_eq!(crashed.summary, "example.net crashed unexpectedly");
        assert!(crashed.open_external.is_some() && crashed.actions_at_end);
    }
}
