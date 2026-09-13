//! Usage and billing settings driven by the connection's account snapshot.

use gpui::{Div, IntoElement, div, prelude::*, px};

use super::SettingsView;
use crate::{
    agent::AgentRateLimitResetCreditStatus,
    components::account::{
        AccountView, credit_expiry_label, credits_balance_label, plan_settings_title,
    },
    theme::Theme,
};

/// One rendered row of the usage page.
struct UsageRow {
    title: String,
    subtitle: Option<String>,
    value: Option<String>,
    button: Option<&'static str>,
    /// Quota rows show the reference usage bar next to their percentage.
    remaining_percent: Option<i32>,
}

/// One rendered section of the usage page.
struct UsageSection {
    title: String,
    subtitle: Option<String>,
    rows: Vec<UsageRow>,
    /// The billing section keeps the reference card spacing.
    balance_card: bool,
}

fn plain_row(title: String, subtitle: Option<String>, value: Option<String>) -> UsageRow {
    UsageRow {
        title,
        subtitle,
        value,
        button: None,
        remaining_percent: None,
    }
}

fn quota_row(row: crate::components::account::RateLimitRow) -> UsageRow {
    UsageRow {
        title: row.title,
        subtitle: row.reset_label,
        value: row.remaining_label,
        button: None,
        remaining_percent: row.remaining_percent,
    }
}

impl AccountView {
    /// Sections the usage page renders. Everything visible is derived from the
    /// account snapshot; unknown values stay labelled as unknown.
    fn usage_sections(&self) -> Vec<UsageSection> {
        let mut sections = Vec::new();

        let plan =
            plan_settings_title(&self.state.account).unwrap_or_else(|| "套餐未知".to_owned());
        sections.push(UsageSection {
            title: "当前套餐".to_owned(),
            subtitle: None,
            rows: vec![UsageRow {
                title: plan,
                subtitle: None,
                value: None,
                button: Some("查看套餐"),
                remaining_percent: None,
            }],
            balance_card: false,
        });

        let (balance, balance_caption) = credits_balance_label(&self.state.rate_limits);
        sections.push(UsageSection {
            title: "额度余额".to_owned(),
            subtitle: Some(
                "购买额度或启用自动充值，达到限额后仍可继续使用 Codex。了解更多".to_owned(),
            ),
            rows: vec![UsageRow {
                title: balance,
                subtitle: Some(balance_caption),
                value: None,
                button: Some("购买额度"),
                remaining_percent: None,
            }],
            balance_card: true,
        });

        let buckets = self.rate_limit_sections();
        if buckets.is_empty() {
            sections.push(UsageSection {
                title: "通用使用限额".to_owned(),
                subtitle: None,
                rows: vec![plain_row(
                    "使用限额".to_owned(),
                    Some(match self.load_error() {
                        Some(error) => error.to_owned(),
                        None if self.is_loading() => "正在读取配额…".to_owned(),
                        None => "后端尚未返回该账户的额度".to_owned(),
                    }),
                    Some("未知".to_owned()),
                )],
                balance_card: false,
            });
        } else {
            for bucket in buckets {
                sections.push(UsageSection {
                    title: bucket.title,
                    subtitle: None,
                    rows: bucket.rows.into_iter().map(quota_row).collect(),
                    balance_card: false,
                });
            }
        }

        if let Some(upsell) = self.upsell_message() {
            sections.push(UsageSection {
                title: "额度提示".to_owned(),
                subtitle: None,
                rows: vec![plain_row(upsell, None, None)],
                balance_card: false,
            });
        }

        let mut reset_rows = Vec::new();
        match self.reset_credits() {
            Some(credits) => {
                reset_rows.push(plain_row(
                    format!("可用 {}", credits.available_count),
                    Some("历史记录".to_owned()),
                    None,
                ));
                for credit in credits.credits.iter().flatten() {
                    // The product names a reset by its kind, so the backend's
                    // own "Full reset" text is not shown verbatim; unknown kinds
                    // still fall back to whatever title the backend supplied.
                    let title = match credit.reset_type {
                        crate::agent::AgentRateLimitResetType::CodexRateLimits => {
                            "完全重置".to_owned()
                        }
                        crate::agent::AgentRateLimitResetType::Unknown => credit
                            .title
                            .clone()
                            .filter(|title| !title.trim().is_empty())
                            .unwrap_or_else(|| "重置额度".to_owned()),
                    };
                    let subtitle = credit_expiry_label(credit.expires_at);
                    let value = match credit.status {
                        AgentRateLimitResetCreditStatus::Available => "可用",
                        AgentRateLimitResetCreditStatus::Redeeming => "兑换中",
                        AgentRateLimitResetCreditStatus::Redeemed => "已兑换",
                        AgentRateLimitResetCreditStatus::Unknown => "状态未知",
                    };
                    reset_rows.push(plain_row(title, Some(subtitle), Some(value.to_owned())));
                }
            }
            None => {
                reset_rows.push(plain_row(
                    "重置额度".to_owned(),
                    Some("后端未返回重置额度".to_owned()),
                    None,
                ));
            }
        }
        sections.push(UsageSection {
            title: "使用限额重置".to_owned(),
            subtitle: None,
            rows: reset_rows,
            balance_card: false,
        });

        sections.push(UsageSection {
            title: "取消套餐".to_owned(),
            subtitle: None,
            rows: vec![plain_row(
                "您的订阅由 ChatGPT 管理。".to_owned(),
                Some("如需取消套餐，请前往账单操作。".to_owned()),
                None,
            )],
            balance_card: false,
        });
        sections
    }
}

impl SettingsView {
    pub(super) fn usage_content(
        &self,
        page: &'static crate::settings::PageSpec,
        theme: Theme,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let account = self.account.clone();
        let is_dark = theme.surface == gpui::rgba(0x181818ff);
        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(96.0))
            .flex()
            .flex_col()
            .gap(px(30.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .relative()
                            .top(px(2.0))
                            .text_size(px(24.0))
                            .line_height(px(28.8))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.text)
                            .child(page.label),
                    )
                    .child(
                        div()
                            .relative()
                            .top(px(2.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(if is_dark {
                                theme.settings_description
                            } else {
                                theme.text_tertiary
                            })
                            .flex()
                            .child("如需查看发票、更改付款方式或进行其他操作，请前往网页版")
                            .child(
                                div()
                                    .text_color(if is_dark {
                                        gpui::rgba(0x99ceffff)
                                    } else {
                                        gpui::rgba(0x339cffff)
                                    })
                                    .child("设置"),
                            )
                            .child({
                                // Quota reads are explicit: the control reports
                                // the in-flight read and the last failure
                                // instead of implying the numbers refreshed
                                // themselves.
                                let error = self.account.load_error().map(str::to_owned);
                                let loading = self.account.is_loading();
                                let button = div()
                                    .id("usage-refresh")
                                    .role(gpui::Role::Button)
                                    .aria_label("刷新配额")
                                    .ml_auto()
                                    .pl(px(16.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(if loading {
                                        theme.settings_description
                                    } else {
                                        theme.accent
                                    })
                                    .when(!loading, |node| {
                                        node.cursor_pointer().on_click(cx.listener(
                                            |_, _, _, cx| cx.emit(crate::settings::RefreshAccount),
                                        ))
                                    })
                                    .child(if loading { "刷新中…" } else { "刷新" });
                                if let Some(error) = error {
                                    button.child(
                                        div()
                                            .ml(px(8.0))
                                            .flex_none()
                                            .max_w(px(320.0))
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.settings_description)
                                            .child(error),
                                    )
                                } else {
                                    button
                                }
                            }),
                    ),
            );
        for (index, section) in account.usage_sections().into_iter().enumerate() {
            content = content.child(
                div()
                    .when(index == 1, |item| item.mt(px(12.0)))
                    .when(index >= 2, |item| item.mt(px(8.0)))
                    .when(index == 0, |item| item.relative().top(px(2.0)))
                    .child(Self::usage_section(section, theme)),
            );
        }
        content.into_any_element()
    }

    fn usage_section(section: UsageSection, theme: Theme) -> Div {
        let mut card = div()
            .id("usage-card")
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        let row_count = section.rows.len();
        for (index, row) in section.rows.into_iter().enumerate() {
            let row_height = if section.balance_card { 61.0 } else { 60.0 };
            card = card.child(
                div()
                    .h(px(row_height))
                    .flex_none()
                    .px(px(16.0))
                    .py(px(12.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 < row_count, |node| {
                        node.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(1.0))
                                .bg(theme.border),
                        )
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .text_color(theme.text)
                                    .child(row.title),
                            )
                            .when_some(row.subtitle, |column, subtitle| {
                                column.child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(if theme.surface == gpui::rgba(0x181818ff) {
                                            theme.settings_description
                                        } else {
                                            theme.text_tertiary
                                        })
                                        .child(subtitle),
                                )
                            }),
                    )
                    .when_some(row.value.or(row.button.map(str::to_owned)), |node, text| {
                        let is_dark = theme.surface == gpui::rgba(0x181818ff);
                        let mut trailing = div().flex_none().flex().items_center().gap(px(10.0));
                        // Reference usage bar: a 95x8 pill whose filled part is
                        // the remaining quota, left of the percentage text.
                        if let Some(percent) = row.remaining_percent {
                            let width = 95.0 * (percent.clamp(0, 100) as f32) / 100.0;
                            trailing = trailing.child(
                                div()
                                    .w(px(95.0))
                                    .h(px(8.0))
                                    .flex_none()
                                    .rounded(px(4.0))
                                    .bg(if is_dark {
                                        gpui::rgba(0x393939ff)
                                    } else {
                                        gpui::rgba(0xe8e8e8ff)
                                    })
                                    .child(div().w(px(width)).h_full().rounded(px(4.0)).bg(
                                        if is_dark {
                                            gpui::rgba(0xffffffff)
                                        } else {
                                            gpui::rgba(0x1a1a1aff)
                                        },
                                    )),
                            );
                        }
                        node.child(
                            trailing.child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .text_color(if is_dark {
                                        theme.settings_description
                                    } else {
                                        theme.text_tertiary
                                    })
                                    .child(text),
                            ),
                        )
                    }),
            );
        }
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(if section.balance_card { 12.0 } else { 16.0 }))
            .child(
                div()
                    .min_h(px(32.0))
                    .when(section.balance_card, |header| {
                        header.h(px(45.0)).flex_none()
                    })
                    .flex()
                    .flex_col()
                    .justify_end()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .text_color(theme.text)
                            .child(section.title),
                    )
                    .when_some(section.subtitle, |header, subtitle| {
                        header.child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(theme.settings_description)
                                .child(subtitle),
                        )
                    }),
            )
            .child(card)
    }
}
