//! The composer's slash command menu, for the commands this client supports:
//! Goal, Compact, Plan mode and the `/autoreview` Approve submenu.
//!
//! As in the reference, the menu opens for a `/query` token that ends at the
//! caret (a `/` at the start of a line or after whitespace; the query may
//! contain spaces), lists commands alphabetically by their localized title,
//! ranks a typed query by fuzzy match on title and id, and closes when a
//! query with a space matches nothing. Compact needs an otherwise empty
//! composer: the whole text must be one `/…` line. Selecting a command removes
//! only the token. Approve only exists while approvable denials do, and opens
//! a submenu of the newest ten.

use std::ops::Range;

use gpui::Context;

use super::{ComposerView, ConversationChanged, toast::ToastKind};
use crate::conversation::AutoApprovalReviewPresentation;

const DENIAL_LIMIT: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlashCommand {
    Approve,
    Compact,
    Goal,
    PlanMode,
}

impl SlashCommand {
    const ALL: [Self; 4] = [Self::Approve, Self::Compact, Self::Goal, Self::PlanMode];

    fn id(self) -> &'static str {
        match self {
            Self::Approve => "autoreview",
            Self::Compact => "compact",
            Self::Goal => "goal",
            Self::PlanMode => "plan-mode",
        }
    }

    pub(crate) fn title(self) -> String {
        match self {
            Self::Approve => crate::i18n::format!("批准" => "Approve"),
            Self::Compact => crate::i18n::format!("压缩" => "Compact"),
            Self::Goal => crate::i18n::format!("目标" => "Goal"),
            Self::PlanMode => crate::i18n::format!("计划模式" => "Plan mode"),
        }
    }

    fn requires_empty_composer(self) -> bool {
        self == Self::Compact
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Approve => "auto-review-shield",
            Self::Compact => "context-compaction",
            Self::Goal => "goal-chip",
            Self::PlanMode => "slash-plan-mode",
        }
    }
}

/// One row of the top-level menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SlashItem {
    pub(crate) command: SlashCommand,
    pub(crate) title: String,
    pub(crate) description: String,
    /// Title byte ranges the query matched; the rest dims while typing.
    pub(crate) matched: Vec<Range<usize>>,
}

/// One row of the Approve submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DenialItem {
    pub(crate) review: AutoApprovalReviewPresentation,
    pub(crate) title: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlashMenu {
    pub(crate) highlighted: usize,
    pub(crate) denials: bool,
    /// Escape closes the menu until the query changes.
    dismissed: Option<String>,
}

/// Matches `query` as a subsequence of `text` (case-insensitive) and scores
/// it: a prefix beats a contiguous match, which beats a scattered one.
fn fuzzy(text: &str, query: &str) -> Option<(u32, Vec<Range<usize>>)> {
    if query.is_empty() {
        return Some((1, Vec::new()));
    }
    let lower = text.to_lowercase();
    // Byte offsets of the lowercase copy only map back when lowercasing kept
    // every length.
    if lower.len() == text.len()
        && let Some(start) = lower.find(query)
    {
        let score = if start == 0 { 30 } else { 20 };
        return Some((score, std::iter::once(start..start + query.len()).collect()));
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut wanted = query.chars().peekable();
    for (index, ch) in text.char_indices() {
        let Some(next) = wanted.peek() else {
            break;
        };
        if ch.to_lowercase().eq(next.to_lowercase()) {
            wanted.next();
            let end = index + ch.len_utf8();
            match ranges.last_mut() {
                Some(last) if last.end == index => last.end = end,
                _ => ranges.push(index..end),
            }
        }
    }
    wanted.peek().is_none().then_some((10, ranges))
}

/// `/gooo` still finds Goal: repeated letters collapse, as the reference
/// normalises the goal query.
fn collapse_repeats(query: &str) -> String {
    let mut out = String::new();
    for ch in query.chars() {
        if !out.ends_with(ch) {
            out.push(ch);
        }
    }
    out
}

/// The `/query` token that ends at `caret`: the nearest `/` on the caret's
/// line that starts the line or follows whitespace. Returns the token's byte
/// range (from the `/` to the caret).
fn slash_token(text: &str, caret: usize) -> Option<Range<usize>> {
    let caret = caret.min(text.len());
    if !text.is_char_boundary(caret) {
        return None;
    }
    let line_start = text[..caret].rfind('\n').map_or(0, |index| index + 1);
    let line = &text[line_start..caret];
    line.char_indices()
        .rev()
        .filter(|(_, ch)| *ch == '/')
        .find(|(index, _)| {
            line[..*index]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
        })
        .map(|(index, _)| line_start + index..caret)
}

/// The reference's "empty composer" for Compact: nothing but one `/…` line.
fn composer_is_only_token(text: &str) -> bool {
    let text = text.trim();
    text.starts_with('/') && !text.contains('\n')
}

impl ComposerView {
    /// The `/query` token at the caret, with its lowercased query.
    fn slash_token(&self, cx: &gpui::App) -> Option<(Range<usize>, String)> {
        let editor = self.prompt_editor.read(cx);
        let range = slash_token(editor.text(), editor.cursor())?;
        let query = editor.text()[range.start + 1..range.end].to_lowercase();
        Some((range, query))
    }

    fn slash_query(&self, cx: &gpui::App) -> Option<String> {
        self.slash_token(cx).map(|(_, query)| query)
    }

    fn command_available(&self, command: SlashCommand) -> bool {
        match command {
            SlashCommand::Approve => !self
                .conversation
                .approvable_denials(DENIAL_LIMIT)
                .is_empty(),
            SlashCommand::Compact => self.conversation.thread_id.is_some() && !self.side_chat,
            SlashCommand::Goal => !self.side_chat,
            SlashCommand::PlanMode => self.plan_mode_available(),
        }
    }

    fn command_description(&self, command: SlashCommand) -> String {
        match command {
            SlashCommand::Approve => {
                crate::i18n::format!("批准最近一次自动审查驳回" => "Approve a recent auto-review denial")
            }
            SlashCommand::Compact => match self.context_usage_percent() {
                Some(usage) => crate::i18n::format!(
                    "压缩此聊天的上下文（已使用 {usage}%）" => "Compact this chat's context ({usage}% full)"
                ),
                None => crate::i18n::format!("压缩此聊天的上下文" => "Compact this chat's context"),
            },
            SlashCommand::Goal => {
                crate::i18n::format!("设置要持续追求的目标" => "Set a goal to keep pursuing")
            }
            SlashCommand::PlanMode => {
                if self.prompt_context.plan_mode == Some(true) {
                    crate::i18n::format!("关闭计划模式" => "Turn plan mode off")
                } else {
                    crate::i18n::format!("开启计划模式" => "Turn plan mode on")
                }
            }
        }
    }

    /// The share of the model context the last turn used, as the reference
    /// computes it: `min(last.totalTokens, window) / window`, rounded.
    fn context_usage_percent(&self) -> Option<i64> {
        let usage = self
            .conversation
            .thread_token_usages
            .get(self.conversation.thread_id.as_deref()?)?;
        let window = usage.model_context_window.filter(|window| *window > 0)?;
        let used = usage.last.total_tokens.clamp(0, window) as f64;
        Some((used * 100.0 / window as f64).round() as i64)
    }

    /// The visible top-level rows for the current query.
    pub(crate) fn slash_items(&self, cx: &gpui::App) -> Vec<SlashItem> {
        let Some(query) = self.slash_query(cx) else {
            return Vec::new();
        };
        let only_token = composer_is_only_token(self.prompt_editor.read(cx).text());
        let mut items = SlashCommand::ALL
            .into_iter()
            .filter(|command| self.command_available(*command))
            .filter(|command| only_token || !command.requires_empty_composer())
            .filter_map(|command| {
                let title = command.title();
                let query = if command == SlashCommand::Goal {
                    collapse_repeats(&query)
                } else {
                    query.clone()
                };
                let by_title = fuzzy(&title, &query);
                let by_id = fuzzy(command.id(), &query).map(|(score, _)| (score, Vec::new()));
                let (score, matched) = match (by_title, by_id) {
                    (Some(title), Some(id)) if id.0 > title.0 => id,
                    (Some(title), _) => title,
                    (None, Some(id)) => id,
                    (None, None) => return None,
                };
                Some((
                    score,
                    SlashItem {
                        command,
                        description: self.command_description(command),
                        title,
                        matched: if query.is_empty() {
                            Vec::new()
                        } else {
                            matched
                        },
                    },
                ))
            })
            .collect::<Vec<_>>();
        items.sort_by(|(a_score, a), (b_score, b)| {
            b_score.cmp(a_score).then_with(|| a.title.cmp(&b.title))
        });
        items.into_iter().map(|(_, item)| item).collect()
    }

    pub(crate) fn denial_items(&self) -> Vec<DenialItem> {
        let approving = self.conversation.approving_review.clone();
        self.conversation
            .approvable_denials(DENIAL_LIMIT)
            .into_iter()
            .map(|review| {
                let detail = if approving.as_deref() == Some(review.review.key.review_id.as_str()) {
                    crate::i18n::format!("正在记录批准操作…" => "Recording approval…")
                } else {
                    review
                        .review
                        .rationale
                        .clone()
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or_else(|| {
                            crate::i18n::format!("自动审查未提供理由" => "Auto-review did not include a rationale")
                        })
                };
                DenialItem {
                    title: crate::components::auto_approval::action_label(&review.review.action),
                    detail,
                    review,
                }
            })
            .collect()
    }

    /// Whether the menu is showing. A query with a space that matches
    /// nothing closes it, so the text can be sent as typed.
    pub(crate) fn slash_menu_open(&self, cx: &gpui::App) -> bool {
        let Some(menu) = &self.slash_menu else {
            return false;
        };
        let Some(query) = self.slash_query(cx) else {
            return false;
        };
        if menu.dismissed.as_ref() == Some(&query) {
            return false;
        }
        menu.denials || !query.contains(char::is_whitespace) || !self.slash_items(cx).is_empty()
    }

    /// Removes the `/query` token, leaving the rest of the composer text.
    fn remove_slash_token(&mut self, cx: &mut Context<Self>) {
        if let Some((range, _)) = self.slash_token(cx) {
            self.prompt_editor
                .update(cx, |editor, cx| editor.replace_range(range, "", cx));
        }
    }

    /// Follows the composer text: a slash token opens the menu, anything else
    /// closes it.
    pub(super) fn update_slash_menu(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.slash_query(cx) else {
            self.slash_menu = None;
            return;
        };
        let menu = self.slash_menu.get_or_insert_with(SlashMenu::default);
        if menu
            .dismissed
            .as_ref()
            .is_some_and(|dismissed| dismissed != &query)
        {
            menu.dismissed = None;
        }
        menu.highlighted = 0;
        menu.denials = false;
        cx.notify();
    }

    fn slash_row_count(&self, cx: &gpui::App) -> usize {
        match &self.slash_menu {
            Some(menu) if menu.denials => self.denial_items().len(),
            _ => self.slash_items(cx).len(),
        }
    }

    /// Up/Down/Enter/Escape while the menu shows. Returns whether the key
    /// was the menu's.
    pub(super) fn slash_menu_key(&mut self, key: &str, ctrl: bool, cx: &mut Context<Self>) -> bool {
        if !self.slash_menu_open(cx) {
            return false;
        }
        let count = self.slash_row_count(cx);
        let query = self.slash_query(cx);
        let Some(menu) = self.slash_menu.as_mut() else {
            return false;
        };
        match (key, ctrl) {
            ("down", false) | ("n", true) if count > 0 => {
                menu.highlighted = (menu.highlighted + 1) % count;
            }
            ("up", false) | ("p", true) if count > 0 => {
                menu.highlighted = (menu.highlighted + count - 1) % count;
            }
            ("escape", false) => {
                if menu.denials {
                    menu.denials = false;
                    menu.highlighted = 0;
                } else {
                    menu.dismissed = query;
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Enter on the menu. Returns whether the menu took it.
    pub(super) fn slash_menu_enter(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.slash_menu_open(cx) {
            return false;
        }
        let Some(menu) = self.slash_menu.clone() else {
            return false;
        };
        if menu.denials {
            self.select_denial(menu.highlighted, cx);
        } else {
            let Some(item) = self.slash_items(cx).into_iter().nth(menu.highlighted) else {
                return true;
            };
            self.select_slash_command(item.command, cx);
        }
        true
    }

    /// Runs a command. The `/query` text is removed first.
    pub(crate) fn select_slash_command(&mut self, command: SlashCommand, cx: &mut Context<Self>) {
        if command == SlashCommand::Approve {
            if let Some(menu) = self.slash_menu.as_mut() {
                menu.denials = true;
                menu.highlighted = 0;
            }
            cx.notify();
            return;
        }
        self.slash_menu = None;
        self.remove_slash_token(cx);
        match command {
            SlashCommand::Goal => {
                if self.prompt_context.plan_mode == Some(true) {
                    self.prompt_context.plan_mode = Some(false);
                }
                self.set_goal_draft(true, cx);
                self.focus_prompt_pending = true;
            }
            SlashCommand::Compact => {
                if self.is_running() {
                    self.show_toast(
                        ToastKind::Danger,
                        crate::i18n::format!("聊天期间无法使用 Compact" => "Compact is disabled while a chat is in progress"),
                        cx,
                    );
                } else {
                    self.start_context_compaction(cx);
                }
            }
            SlashCommand::PlanMode => {
                let on = self.prompt_context.plan_mode != Some(true);
                self.prompt_context.plan_mode = Some(on);
                self.focus_prompt_pending = true;
            }
            SlashCommand::Approve => unreachable!("opens its submenu above"),
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    /// Approves one denial from the submenu. The menu stays open while it
    /// records, and closes once the approval lands.
    pub(crate) fn select_denial(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.conversation.approving_review.is_some() {
            return;
        }
        let Some(item) = self.denial_items().into_iter().nth(index) else {
            return;
        };
        self.approve_review(item.review.review.key.clone(), cx);
    }

    /// A recorded approval closes the Approve submenu.
    pub(super) fn close_denial_menu(&mut self, cx: &mut Context<Self>) {
        if self.slash_menu.as_ref().is_some_and(|menu| menu.denials) {
            self.remove_slash_token(cx);
            self.slash_menu = None;
        }
    }

    pub(crate) fn denial_approval_in_flight(&self) -> bool {
        self.conversation.approving_review.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{collapse_repeats, composer_is_only_token, fuzzy, slash_token};

    #[test]
    fn the_token_starts_at_a_line_start_or_after_whitespace_and_ends_at_the_caret() {
        assert_eq!(slash_token("/go", 3), Some(0..3));
        assert_eq!(slash_token("fix this /go", 12), Some(9..12));
        assert_eq!(slash_token("and/or", 6), None, "not after a letter");
        assert_eq!(
            slash_token("/goal and/or", 12),
            Some(0..12),
            "spaces stay in the query"
        );
        assert_eq!(slash_token("line\n/co", 8), Some(5..8));
        assert_eq!(slash_token("/co\nnext", 9), None, "the caret's line only");
        assert_eq!(
            slash_token("/compact rest", 3),
            Some(0..3),
            "up to the caret"
        );
        assert!(composer_is_only_token("/compact foo"));
        assert!(!composer_is_only_token("hello /compact"));
    }

    #[test]
    fn fuzzy_prefers_prefixes_and_marks_the_matched_title_parts() {
        assert_eq!(
            fuzzy("Goal", "go"),
            Some((30, std::iter::once(0..2).collect()))
        );
        assert_eq!(
            fuzzy("Plan mode", "mode"),
            Some((20, std::iter::once(5..9).collect()))
        );
        assert_eq!(fuzzy("Plan mode", "pm"), Some((10, vec![0..1, 5..6])));
        assert_eq!(fuzzy("Compact", "x"), None);
        assert_eq!(collapse_repeats("gooo"), "go");
    }
}
