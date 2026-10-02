//! What the address bar does with typed text, and how it shows a page's URL.
//!
//! Ported from ChatGPT 26.924's in-app browser (`app-shared`): text with a
//! scheme or `about:` is kept, an absolute path becomes a `file:` URL, a
//! loopback host gets `http://`, a host with a port, `www.`, an IP address or a
//! registered top-level domain gets `https://`, and anything else is a Google
//! search.

use std::{collections::HashSet, sync::OnceLock};

use url::Url;

const SEARCH_URL: &str = "https://www.google.com/search";

/// Hosts the reference treats as this machine (`localhost`, loopback and the
/// unspecified address), plus any `*.localhost` name.
const LOCAL_HOSTS: [&str; 5] = ["localhost", "127.0.0.1", "0.0.0.0", "[::1]", "::1"];

/// The URL to load for text typed into the address bar. Empty input stays
/// empty.
pub fn navigation_url(input: &str) -> String {
    let text = input.trim();
    if text.is_empty() {
        return String::new();
    }
    if has_scheme(text) || text.to_ascii_lowercase().starts_with("about:") {
        return text.to_owned();
    }
    if text.starts_with('/') && !text.starts_with("//") && !input.contains('\n') {
        return file_url(text);
    }
    let host = typed_host(text);
    if host
        .as_ref()
        .is_some_and(|host| is_local_host(&host.hostname))
    {
        return format!("http://{text}");
    }
    if let Some(host) = host
        && !text.chars().any(char::is_whitespace)
        && (host.has_port
            || host.hostname.starts_with("www.")
            || is_ip_address(&host.hostname)
            || has_registered_domain(&host.hostname))
    {
        return format!("https://{text}");
    }
    search_url(text)
}

/// True when the address bar would search the web for this text rather than
/// open it as an address.
pub fn is_search(input: &str) -> bool {
    let text = input.trim();
    !text.is_empty() && navigation_url(input) == search_url(text)
}

pub fn search_url(query: &str) -> String {
    let mut url = Url::parse(SEARCH_URL).expect("search URL is valid");
    url.query_pairs_mut().append_pair("q", query);
    url.into()
}

/// The address bar's resting text for a page: a local file's path, a local
/// server's URL without its scheme, an https page's host without `www.`, and
/// an http page's host with its scheme.
pub fn display_text(url: &str) -> String {
    let text = url.trim();
    if text.is_empty() {
        return String::new();
    }
    let Ok(parsed) = Url::parse(text) else {
        return text.to_owned();
    };
    match parsed.scheme() {
        "file" => {
            let path = percent_decode(parsed.path());
            match parsed.host_str() {
                Some(host) if !host.is_empty() => format!("//{host}{path}"),
                _ => path,
            }
        }
        "http" | "https" if parsed.host_str().is_some_and(is_local_host) => strip_scheme(&parsed),
        "https" => parsed
            .host_str()
            .map(|host| host.strip_prefix("www.").unwrap_or(host).to_owned())
            .unwrap_or_else(|| text.to_owned()),
        "http" => parsed
            .host_str()
            .map(|host| format!("http://{host}"))
            .unwrap_or_else(|| text.to_owned()),
        _ => text.to_owned(),
    }
}

/// The text an address-bar suggestion row and inline completion work with:
/// the URL without its scheme or a trailing root slash.
pub fn completion_text(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => strip_scheme(&parsed),
        _ => url.to_owned(),
    }
}

/// The host a page belongs to, for history grouping and error pages.
pub fn host(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    match parsed.host_str() {
        Some(host) if !host.is_empty() => Some(host.to_owned()),
        _ => (!parsed.path().is_empty()).then(|| parsed.path().to_owned()),
    }
}

/// Whether an http(s) page may be handed to the system browser.
pub fn is_web_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
}

/// A URL is local when it points at this machine: a loopback web server or a
/// file on disk. The reference keeps a separate "open local links in" choice.
pub fn is_local_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "http" | "https" => parsed.host_str().is_some_and(is_local_host),
        "file" => parsed
            .host_str()
            .is_none_or(|host| host.is_empty() || host == "localhost"),
        _ => false,
    }
}

fn strip_scheme(url: &Url) -> String {
    let text = url.as_str();
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(text);
    if url.path() == "/" && url.query().is_none() && url.fragment().is_none() {
        rest.strip_suffix('/').unwrap_or(rest).to_owned()
    } else {
        rest.to_owned()
    }
}

fn has_scheme(text: &str) -> bool {
    let Some((scheme, _)) = text.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

fn file_url(path: &str) -> String {
    let mut url = Url::parse("file:///").expect("file URL is valid");
    url.set_path(path);
    url.into()
}

fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| path.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

struct TypedHost {
    hostname: String,
    has_port: bool,
}

/// The host part of typed text, up to the first `/`, `?` or `#`. Text with
/// user info, or a colon that is not a port, has no host.
fn typed_host(text: &str) -> Option<TypedHost> {
    let end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let authority = &text[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let close = rest.find(']')?;
        let hostname = format!("[{}]", &rest[..close]);
        let after = &rest[close + 1..];
        let has_port = match after.strip_prefix(':') {
            Some(port) => !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()),
            None if after.is_empty() => false,
            None => return None,
        };
        return Some(TypedHost { hostname, has_port });
    }
    match authority.split_once(':') {
        Some((hostname, port))
            if !hostname.is_empty()
                && !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit()) =>
        {
            Some(TypedHost {
                hostname: hostname.to_owned(),
                has_port: true,
            })
        }
        Some(_) => None,
        None => Some(TypedHost {
            hostname: authority.to_owned(),
            has_port: false,
        }),
    }
}

fn is_local_host(hostname: &str) -> bool {
    let host = hostname.to_ascii_lowercase();
    host.ends_with(".localhost") || LOCAL_HOSTS.contains(&host.as_str())
}

fn is_ip_address(hostname: &str) -> bool {
    if hostname.starts_with('[') {
        return Url::parse(&format!("https://{hostname}")).is_ok();
    }
    let parts: Vec<&str> = hostname.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<u16>().is_ok_and(|value| value <= 255)
        })
}

/// A host under a top-level domain from the IANA root zone, with at least one
/// label before it, as the reference's public-suffix check requires.
fn has_registered_domain(hostname: &str) -> bool {
    let Ok(parsed) = Url::parse(&format!("https://{hostname}/")) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_end_matches('.');
    let Some((before, tld)) = host.rsplit_once('.') else {
        return false;
    };
    !before.is_empty() && !before.ends_with('.') && top_level_domains().contains(tld)
}

fn top_level_domains() -> &'static HashSet<&'static str> {
    static DOMAINS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    DOMAINS.get_or_init(|| {
        include_str!("tlds.txt")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_becomes_the_reference_navigation_url() {
        let cases = [
            ("", ""),
            ("  ", ""),
            ("https://example.com/a", "https://example.com/a"),
            ("chrome-extension://abc/x", "chrome-extension://abc/x"),
            ("about:blank", "about:blank"),
            ("example.com", "https://example.com"),
            ("example.net/path?q=1", "https://example.net/path?q=1"),
            ("www.intranet", "https://www.intranet"),
            ("localhost:3000", "http://localhost:3000"),
            ("app.localhost/x", "http://app.localhost/x"),
            ("127.0.0.1:8080/x", "http://127.0.0.1:8080/x"),
            ("192.168.1.10", "https://192.168.1.10"),
            ("intranet:8080", "https://intranet:8080"),
            ("[2001:db8::1]:443", "https://[2001:db8::1]:443"),
            ("docs.rs/url", "https://docs.rs/url"),
            ("github.io", "https://github.io"),
            ("/Users/me/index.html", "file:///Users/me/index.html"),
            ("/tmp/a b.html", "file:///tmp/a%20b.html"),
        ];
        for (input, expected) in cases {
            assert_eq!(navigation_url(input), expected, "{input:?}");
        }
    }

    #[test]
    fn everything_else_searches_google() {
        for input in [
            "example",
            "rust gpui",
            "foo.notatld",
            "com",
            "user@example.com",
            "what is example.com",
        ] {
            assert_eq!(navigation_url(input), search_url(input.trim()), "{input:?}");
            assert!(is_search(input), "{input:?}");
        }
        assert_eq!(
            search_url("a b&c"),
            "https://www.google.com/search?q=a+b%26c"
        );
        assert!(!is_search("example.com"));
    }

    #[test]
    fn display_text_matches_the_reference_address_bar() {
        let cases = [
            ("https://www.example.com/a?b", "example.com"),
            ("https://example.net/", "example.net"),
            ("http://example.org/page", "http://example.org"),
            ("http://localhost:3000/", "localhost:3000"),
            ("http://localhost:3000/app?x=1", "localhost:3000/app?x=1"),
            ("file:///Users/me/a%20b.html", "/Users/me/a b.html"),
            ("about:blank", "about:blank"),
        ];
        for (url, expected) in cases {
            assert_eq!(display_text(url), expected, "{url:?}");
        }
    }

    #[test]
    fn completion_text_drops_the_scheme_and_root_slash() {
        assert_eq!(completion_text("https://example.net/"), "example.net");
        assert_eq!(
            completion_text("https://www.example.net/a/"),
            "www.example.net/a/"
        );
        assert_eq!(completion_text("file:///tmp/a"), "file:///tmp/a");
    }

    #[test]
    fn local_and_web_urls() {
        assert!(is_local_url("http://localhost:5173/"));
        assert!(is_local_url("file:///tmp/a.html"));
        assert!(!is_local_url("https://example.com/"));
        assert!(is_web_url("https://example.com/"));
        assert!(!is_web_url("file:///tmp/a.html"));
        assert_eq!(
            host("https://sub.example.com/x").as_deref(),
            Some("sub.example.com")
        );
    }
}
