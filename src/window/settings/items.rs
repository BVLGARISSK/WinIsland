use crate::utils::settings_ui::content_height;
use crate::utils::settings_ui::items::SettingsItem;

use super::{BuiltinPage, SETTINGS_HEADER_H, SettingsApp};

impl SettingsApp {
    pub(crate) fn build_current_items(&self) -> Vec<SettingsItem> {
        match BuiltinPage::from_index(self.active_page) {
            Some(BuiltinPage::General) => self.build_general_page().into_items(),
            Some(BuiltinPage::Appearance) => self.build_appearance_page().into_items(),
            Some(BuiltinPage::Layout) => self.build_layout_page().into_items(),
            Some(BuiltinPage::Behavior) => self.build_behavior_page().into_items(),
            Some(BuiltinPage::Alerts) => self.build_alerts_page().into_items(),
            Some(BuiltinPage::Music) => self.build_music_items(),
            Some(BuiltinPage::Widgets) => self.build_widget_items(),
            Some(BuiltinPage::Plugins) => self.build_plugin_items(),
            Some(BuiltinPage::LingoSoul) => self.build_lingosoul_items(),
            Some(BuiltinPage::About) => self.build_about_items(),
            None => self.build_plugin_settings_items(),
        }
    }

    pub(crate) fn rebuild_items_cache(&mut self) {
        self.cached_items = self.build_current_items();
        let switch_states: Vec<bool> = self
            .cached_items
            .iter()
            .filter_map(|item| match item {
                SettingsItem::RowSwitch { on, .. } => Some(*on),
                _ => None,
            })
            .collect();
        let switch_context = (self.active_page, 0);
        if self.switch_anim_context != switch_context
            || self.switch_anim.len() != switch_states.len()
        {
            self.switch_anim = crate::utils::settings_ui::SwitchAnimator::new(&switch_states);
            self.switch_anim_context = switch_context;
        } else {
            self.switch_anim.set_targets(&switch_states);
        }
        self.cached_content_height = content_height(&self.cached_items, SETTINGS_HEADER_H);
        let view_h = self.logical_window_size().1;
        self.cached_max_scroll = (self.cached_content_height - view_h + 20.0).max(0.0);
        self.items_dirty = false;
    }

    pub(crate) fn ensure_items_cache(&mut self) {
        if self.items_dirty {
            self.rebuild_items_cache();
        }
    }

    pub(crate) fn mark_items_dirty(&mut self) {
        self.items_dirty = true;
    }
}
