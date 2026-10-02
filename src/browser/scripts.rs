//! Scripts the in-app browser injects into pages. They only report to the app
//! through the `echora` message handler and answer find requests.

/// Main frame, document end: the page icon, re-reported when `<head>` changes.
pub const PAGE_SCRIPT: &str = r#"(() => {
  if (window.__echoraPage) return;
  window.__echoraPage = true;
  const post = (message) => {
    try { window.webkit.messageHandlers.echora.postMessage(message); } catch (_) {}
  };
  let lastIcon = null;
  const reportIcon = () => {
    const links = [...document.querySelectorAll('link[rel]')].filter((link) =>
      /(^|\s)(shortcut\s+)?icon(\s|$)/i.test(link.rel) || /apple-touch-icon/i.test(link.rel));
    const preferred = links.find((link) => /(^|\s)icon(\s|$)/i.test(link.rel) && /svg|png/i.test(link.type || link.href))
      || links.find((link) => /(^|\s)icon(\s|$)/i.test(link.rel))
      || links[0];
    let href = preferred ? preferred.href : '';
    if (!href && /^https?:$/.test(location.protocol)) href = new URL('/favicon.ico', location.href).href;
    if (href === lastIcon) return;
    lastIcon = href;
    post({ type: 'favicon', href, page: location.href });
  };
  reportIcon();
  let pending = false;
  new MutationObserver(() => {
    if (pending) return;
    pending = true;
    setTimeout(() => { pending = false; reportIcon(); }, 250);
  }).observe(document.head || document.documentElement, {
    childList: true, subtree: true, attributes: true, attributeFilter: ['href', 'rel'],
  });

  const MAX_MATCHES = 1000;
  const highlights = typeof CSS !== 'undefined' && CSS.highlights && typeof Highlight !== 'undefined';
  let ranges = [];
  let active = -1;
  let lastQuery = '';
  let style = null;
  const collect = (query) => {
    ranges = [];
    if (!query || !document.body) return;
    const needle = query.toLocaleLowerCase();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        if (/^(SCRIPT|STYLE|NOSCRIPT|TEMPLATE)$/.test(parent.tagName)) return NodeFilter.FILTER_REJECT;
        if (parent.getClientRects().length === 0) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    while (walker.nextNode() && ranges.length < MAX_MATCHES) {
      const node = walker.currentNode;
      const text = node.data.toLocaleLowerCase();
      let index = text.indexOf(needle);
      while (index !== -1 && ranges.length < MAX_MATCHES) {
        const range = document.createRange();
        range.setStart(node, index);
        range.setEnd(node, index + query.length);
        ranges.push(range);
        index = text.indexOf(needle, index + query.length);
      }
    }
  };
  const paint = () => {
    if (!highlights) return;
    if (!style) {
      style = document.createElement('style');
      style.textContent = '::highlight(echora-find){background-color:#ffff00;color:#000}' +
        '::highlight(echora-find-active){background-color:#ff9632;color:#000}';
      (document.head || document.documentElement).appendChild(style);
    }
    CSS.highlights.set('echora-find', new Highlight(...ranges));
    CSS.highlights.set('echora-find-active', active >= 0 ? new Highlight(ranges[active]) : new Highlight());
  };
  const reveal = () => {
    const range = ranges[active];
    if (!range) return;
    const rect = range.getBoundingClientRect();
    if (rect.top < 0 || rect.bottom > innerHeight || rect.left < 0 || rect.right > innerWidth) {
      const element = range.startContainer.parentElement;
      if (element) element.scrollIntoView({ block: 'center', inline: 'nearest' });
    }
    if (!highlights) {
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
  };
  window.__echoraFind = {
    find(query, backwards) {
      if (query !== lastQuery) {
        lastQuery = query;
        collect(query);
        active = ranges.length ? 0 : -1;
      } else if (ranges.length) {
        active = (active + (backwards ? -1 : 1) + ranges.length) % ranges.length;
      }
      paint();
      reveal();
      return JSON.stringify({ matches: ranges.length, active: active + 1 });
    },
    clear() {
      ranges = [];
      active = -1;
      lastQuery = '';
      if (highlights) {
        CSS.highlights.delete('echora-find');
        CSS.highlights.delete('echora-find-active');
      }
      return JSON.stringify({ matches: 0, active: 0 });
    },
  };
})();"#;

/// Every frame, document start: the link under a right click, read by the
/// native context menu that opens right after.
pub const CONTEXT_SCRIPT: &str = r#"(() => {
  if (window.__echoraContext) return;
  window.__echoraContext = true;
  document.addEventListener('contextmenu', (event) => {
    const path = event.composedPath ? event.composedPath() : [];
    const anchor = path.find((node) => node instanceof Element && node.closest && node.closest('a[href]'));
    const link = anchor ? anchor.closest('a[href]').href : '';
    try { window.webkit.messageHandlers.echora.postMessage({ type: 'contextmenu', link }); } catch (_) {}
  }, true);
})();"#;

/// Runs the find script and returns its JSON answer.
pub fn find_call(query: &str, backwards: bool) -> String {
    let query = serde_json::to_string(query).unwrap_or_else(|_| "\"\"".into());
    format!(
        "window.__echoraFind ? window.__echoraFind.find({query}, {backwards}) : JSON.stringify({{matches:0,active:0}})"
    )
}

pub const CLEAR_FIND_CALL: &str = "window.__echoraFind && window.__echoraFind.clear()";

/// Parses the find script's `{matches, active}` answer.
pub fn parse_find_result(json: &str) -> super::webview::FindResult {
    #[derive(serde::Deserialize)]
    struct Answer {
        matches: usize,
        active: usize,
    }
    serde_json::from_str::<Answer>(json)
        .map(|answer| super::webview::FindResult {
            matches: answer.matches,
            active: answer.active.min(answer.matches),
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_calls_quote_the_query_and_answers_parse() {
        assert_eq!(
            find_call("a\"b", true),
            "window.__echoraFind ? window.__echoraFind.find(\"a\\\"b\", true) : JSON.stringify({matches:0,active:0})"
        );
        let result = parse_find_result(r#"{"matches":3,"active":2}"#);
        assert_eq!((result.matches, result.active), (3, 2));
        assert_eq!(parse_find_result("not json").matches, 0);
    }
}
