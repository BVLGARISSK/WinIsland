use winisland_core::config::AppConfigField;
use winisland_core::i18n::tr;

use super::super::{PageInput, SettingsPage};
use super::SettingsApp;

impl SettingsApp {
    pub(crate) fn build_alerts_page(&self) -> SettingsPage<()> {
        let caps = crate::platform::capabilities();
        let mut page = SettingsPage::new();
        page.section(tr("section_notifications"));
        page.group_start();
        page.setting(
            &self.config,
            AppConfigField::NotificationDisplay,
            caps.toast_events,
        );
        if !caps.toast_events {
            page.row_label(tr("platform_unavailable"));
        }
        page.group_end();

        page.section(tr("section_system_alerts"));
        page.group_start();
        let overlay_available =
            (caps.input_hooks && caps.volume_control) || caps.brightness_control;
        page.setting(
            &self.config,
            AppConfigField::SystemControlsOverlay,
            overlay_available,
        );
        page.setting(&self.config, AppConfigField::DeviceStatusIndicators, true);
        page.setting(&self.config, AppConfigField::ClipboardLinkPrompt, true);
        if !overlay_available {
            page.row_label(tr("platform_unavailable"));
        }
        page.group_end();
        page
    }

    pub(crate) fn handle_alerts_click(&mut self, input: PageInput) {
        let page = self.build_alerts_page();
        let result = input.hit_test(&page);
        self.handle_setting_click(&page, &result, input);
    }
}
