use crate::utils::settings_ui::ClickResult;
use crate::window::settings::PopupState;
use winisland_core::config::AppConfigField;
use winisland_core::i18n::tr;

use super::super::{PageInput, SettingsPage};
use super::SettingsApp;

#[derive(Clone, Copy)]
pub(crate) enum LayoutAction {
    Monitor,
}

impl SettingsApp {
    pub(crate) fn build_layout_page(&self) -> SettingsPage<LayoutAction> {
        let mut page = SettingsPage::new();
        page.section(tr("section_position"));
        page.group_start();
        let monitors = Self::get_monitor_list();
        let selected_monitor =
            (self.config.monitor_index as usize).min(monitors.len().saturating_sub(1));
        page.row_source(
            tr("monitor"),
            monitors
                .into_iter()
                .enumerate()
                .map(|(index, name)| (name, index == selected_monitor))
                .collect(),
            true,
            LayoutAction::Monitor,
        );
        page.setting(&self.config, AppConfigField::PositionXOffset, true);
        page.setting(&self.config, AppConfigField::PositionYOffset, true);
        page.group_end();

        page.section(tr("section_compact_size"));
        page.group_start();
        page.setting(&self.config, AppConfigField::CompactScale, true);
        page.setting(&self.config, AppConfigField::BaseWidth, true);
        page.setting(&self.config, AppConfigField::BaseHeight, true);
        page.group_end();

        page.section(tr("section_expanded_size"));
        page.group_start();
        page.setting(&self.config, AppConfigField::ExpandedScale, true);
        page.setting(&self.config, AppConfigField::ExpandedWidth, true);
        page.setting(&self.config, AppConfigField::ExpandedHeight, true);
        page.group_end();
        page
    }

    pub(crate) fn handle_layout_click(&mut self, input: PageInput) {
        let page = self.build_layout_page();
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        let (Some(LayoutAction::Monitor), ClickResult::SourceButton(item_index)) =
            (page.action(&result), result)
        else {
            return;
        };
        let button_rect = input.popup_button_rect(&page, item_index, self.scroll_y);
        let (win_w, win_h) = self.logical_window_size();

        let monitors = Self::get_monitor_list();
        let selected = (self.config.monitor_index as usize).min(monitors.len().saturating_sub(1));
        let values = (0..monitors.len()).map(|index| index.to_string()).collect();
        self.show_popup(PopupState::new(
            select_monitor,
            button_rect,
            monitors,
            values,
            selected,
            win_w,
            win_h,
        ));
    }
}

fn select_monitor(app: &mut SettingsApp, value: &str) {
    app.config.monitor_index = value.parse().unwrap_or(0);
}
