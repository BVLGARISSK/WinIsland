use crate::core::assistant::{local_parts, read_history};
use crate::utils::settings_ui::ClickResult;
use crate::utils::settings_ui::items::SettingsItem;
use winisland_core::config::AppConfigField;
use winisland_core::i18n::tr;

use super::super::{PluginSettingsRequest, SettingsApp};
use super::{PageInput, SettingsPage};

const PREVIEW_CHARS: usize = 80;

#[derive(Clone, Copy)]
pub(crate) enum LingoSoulAction {
    Clear,
}

/// One line of a message for the list: no Markdown marks, cut to a preview.
fn preview(text: &str) -> String {
    let flat = text
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '-', '*', '>', '|', ' '])
        })
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace(['*', '`', '|'], "");
    if flat.chars().count() <= PREVIEW_CHARS {
        flat
    } else {
        flat.chars().take(PREVIEW_CHARS).collect::<String>() + "…"
    }
}

impl SettingsApp {
    pub(crate) fn build_lingosoul_page(&self, with_history: bool) -> SettingsPage<LingoSoulAction> {
        let mut page = SettingsPage::new();
        let history = if with_history {
            read_history()
        } else {
            Vec::new()
        };
        page.section(tr("lingosoul_history_title"));
        page.group_start();
        page.setting(
            &self.config,
            AppConfigField::LingosoulHistoryRetention,
            true,
        );
        page.row_button(
            tr("lingosoul_history_clear"),
            tr("lingosoul_chat_clear"),
            !with_history || !history.is_empty(),
            LingoSoulAction::Clear,
        );
        page.group_end();
        if !with_history {
            return page;
        }
        if history.is_empty() {
            page.group_start();
            page.row_label(tr("lingosoul_history_empty"));
            page.group_end();
            return page;
        }
        let you = tr("lingosoul_history_you");
        let assistant = tr("lingosoul_chat_title");
        let mut days: Vec<((i64, u32, u32), Vec<String>)> = Vec::new();
        for message in &history {
            let (year, month, day, hour, minute) = local_parts(message.at_ms);
            let row = format!(
                "{hour:02}:{minute:02}   {}：{}",
                if message.from_user { &you } else { &assistant },
                preview(&message.text)
            );
            match days.last_mut() {
                Some((key, rows)) if *key == (year, month, day) => rows.push(row),
                _ => days.push(((year, month, day), vec![row])),
            }
        }
        for ((year, month, day), rows) in days.into_iter().rev() {
            page.section(format!("{year}-{month:02}-{day:02}"));
            page.group_start();
            for row in rows {
                page.row_label(row);
            }
            page.group_end();
        }
        page
    }

    pub(crate) fn build_lingosoul_items(&self) -> Vec<SettingsItem> {
        self.build_lingosoul_page(true).into_items()
    }

    pub(crate) fn handle_lingosoul_click(&mut self, input: PageInput) {
        let page = self.build_lingosoul_page(true);
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        if let (Some(LingoSoulAction::Clear), ClickResult::RowButton(_)) =
            (page.action(&result).copied(), &result)
        {
            self.plugin_request = Some(PluginSettingsRequest::ClearLingoSoulHistory);
        }
    }
}
