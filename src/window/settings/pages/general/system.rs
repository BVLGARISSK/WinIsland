use crate::utils::settings_ui::ClickResult;
use crate::window::settings::PopupState;
use winisland_core::config::{AppConfig, AppConfigField};
use winisland_core::i18n::{available_langs, current_lang, init_i18n, set_lang, tr};

use super::super::{PageInput, SettingsPage};
use super::SettingsApp;

#[derive(Clone, Copy)]
pub(crate) enum GeneralAction {
    AutoStart,
    Language,
    CheckUpdatesNow,
    ResetDefaults,
    HideIsland,
    Exit,
}

impl SettingsApp {
    pub(crate) fn build_general_page(&self) -> SettingsPage<GeneralAction> {
        let caps = crate::platform::capabilities();
        let mut page = SettingsPage::new();
        page.section(tr("section_system"));
        page.group_start();
        page.row_switch(
            tr("start_boot"),
            self.config.auto_start,
            caps.autostart,
            GeneralAction::AutoStart,
        );
        let language = current_lang();
        page.row_source(
            tr("language"),
            available_langs()
                .iter()
                .map(|entry| (entry.name.clone(), entry.code == language))
                .collect(),
            true,
            GeneralAction::Language,
        );
        page.setting(&self.config, AppConfigField::SettingsTheme, true);
        if !caps.autostart {
            page.row_label(tr("platform_unavailable"));
        }
        if !caps.tray {
            page.row_button(
                tr("tray_hide"),
                tr("tray_hide"),
                true,
                GeneralAction::HideIsland,
            );
            page.row_button(tr("tray_exit"), tr("tray_exit"), true, GeneralAction::Exit);
        }
        page.group_end();

        page.section(tr("section_updates"));
        page.group_start();
        page.setting(&self.config, AppConfigField::CheckForUpdates, true);
        if self.config.check_for_updates {
            page.setting(&self.config, AppConfigField::UpdateChannel, true);
            page.setting(&self.config, AppConfigField::UpdateCheckInterval, true);
        }
        page.row_button(
            tr("check_updates_manual"),
            tr("update_check_btn"),
            true,
            GeneralAction::CheckUpdatesNow,
        );
        page.group_end();
        page.spacer(10.0);
        page.center_link(
            tr("reset_defaults"),
            self.theme().danger,
            GeneralAction::ResetDefaults,
        );
        page
    }

    pub(crate) fn handle_general_click(&mut self, input: PageInput) {
        let page = self.build_general_page();
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        let Some(action) = page.action(&result).copied() else {
            return;
        };

        let changed = match (action, &result) {
            (GeneralAction::AutoStart, ClickResult::Switch(_)) => {
                let enabled = !self.config.auto_start;
                match crate::platform::shell().set_autostart(enabled) {
                    Ok(()) => {
                        self.config.auto_start = enabled;
                        true
                    }
                    Err(error) => {
                        crate::platform::update_capabilities(|caps| caps.autostart = false);
                        log::warn!("Autostart is unavailable: {error}");
                        false
                    }
                }
            }
            (GeneralAction::HideIsland, ClickResult::RowButton(_)) => {
                self.plugin_request =
                    Some(crate::window::settings::PluginSettingsRequest::HideIsland);
                false
            }
            (GeneralAction::Exit, ClickResult::RowButton(_)) => {
                self.plugin_request = Some(crate::window::settings::PluginSettingsRequest::Exit);
                false
            }
            (GeneralAction::ResetDefaults, ClickResult::CenterLink(_)) => {
                self.config = AppConfig::default();
                init_i18n(&self.config.language);
                crate::ui::widget::expanded::calendar::clear_calendar_text_cache();
                crate::utils::fonts::apply(&self.config);
                true
            }
            (GeneralAction::CheckUpdatesNow, ClickResult::RowButton(_)) => {
                crate::utils::updater::check_updates_manually();
                false
            }
            _ => false,
        };
        if changed {
            self.persist_settings_change();
            return;
        }

        let (GeneralAction::Language, ClickResult::SourceButton(item_index)) = (action, result)
        else {
            return;
        };
        let button_rect = input.popup_button_rect(&page, item_index, self.scroll_y);
        let (win_w, win_h) = self.logical_window_size();
        let languages = available_langs();
        let current = current_lang();
        let selected = languages
            .iter()
            .position(|entry| entry.code == current)
            .unwrap_or(0);
        self.show_popup(PopupState::new(
            select_language,
            button_rect,
            languages.iter().map(|entry| entry.name.clone()).collect(),
            languages.iter().map(|entry| entry.code.clone()).collect(),
            selected,
            win_w,
            win_h,
        ));
    }
}

fn select_language(app: &mut SettingsApp, value: &str) {
    app.config.language = value.to_string();
    set_lang(value);
    crate::ui::widget::expanded::calendar::clear_calendar_text_cache();
    crate::utils::fonts::apply(&app.config);
}
