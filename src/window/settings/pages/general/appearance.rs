use crate::utils::settings_ui::ClickResult;
use crate::window::settings::PopupState;
use winisland_core::config::AppConfigField;
use winisland_core::i18n::{tr, tr_args};
use winisland_render::text::FontManager;

use super::super::{PageInput, SettingsPage};
use super::SettingsApp;

const FONT_FILE_CHOICE: &str = "\u{1}file";

#[derive(Clone, Copy)]
pub(crate) enum AppearanceAction {
    IslandStyle,
    FontFamily,
    RecommendedFont,
}

impl SettingsApp {
    fn font_family_label(&self) -> String {
        if let Some(path) = &self.config.custom_font_path {
            return FontManager::global()
                .custom_family_name()
                .unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone())
                });
        }
        self.config
            .font_family
            .clone()
            .unwrap_or_else(|| tr("font_system_default"))
    }

    pub(crate) fn build_appearance_page(&self) -> SettingsPage<AppearanceAction> {
        let mut page = SettingsPage::new();
        page.section(tr("section_island_style"));
        page.group_start();
        let host_backdrop = crate::platform::capabilities().host_backdrop;
        let style = if !host_backdrop && self.config.island_style == "glass" {
            "dynamic"
        } else {
            self.config.island_style.as_str()
        };
        let mut styles = vec![(tr("style_default"), style == "default")];
        if host_backdrop {
            styles.push((tr("style_glass"), style == "glass"));
        }
        styles.push((tr("style_dynamic"), style == "dynamic"));
        styles.push((tr("style_liquid_glass"), style == "liquid"));
        page.row_source(
            tr("island_style"),
            styles,
            true,
            AppearanceAction::IslandStyle,
        );
        if style == "glass" {
            page.setting(&self.config, AppConfigField::GlassBlur, true);
        }
        if style == "liquid" {
            page.setting(&self.config, AppConfigField::LiquidGlassBlur, true);
            page.setting(&self.config, AppConfigField::LiquidGlassRefraction, true);
            page.setting(&self.config, AppConfigField::LiquidGlassDispersion, true);
            page.setting(&self.config, AppConfigField::LiquidGlassTint, true);
            page.setting(&self.config, AppConfigField::LiquidGlassSiri, true);
            page.row_label(tr("liquid_glass_siri_note"));
            page.setting(&self.config, AppConfigField::LiquidGlassCompatCapture, true);
            page.row_label(tr(if self.config.liquid_glass_compat_capture {
                "liquid_glass_compat_capture_note"
            } else {
                "style_liquid_note"
            }));
        }
        if !host_backdrop {
            page.row_label(tr("platform_unavailable"));
        }
        page.group_end();

        page.section(tr("section_font"));
        page.group_start();
        page.row_source(
            tr("font_family"),
            vec![(self.font_family_label(), true)],
            true,
            AppearanceAction::FontFamily,
        );
        if self.config.custom_font_path.is_none()
            && let Some(family) = &self.config.font_family
        {
            let resolved = crate::utils::fonts::resolve_family(family);
            if &resolved != family {
                page.row_label(tr_args("font_region_note", &[&resolved]));
            }
        }
        page.setting(&self.config, AppConfigField::FontWeight, true);
        page.setting(&self.config, AppConfigField::FontSize, true);
        page.group_end();
        page.group_start();
        page.row_button(
            tr("font_recommended"),
            tr("font_download"),
            true,
            AppearanceAction::RecommendedFont,
        );
        page.row_label(tr("font_recommended_note"));
        page.group_end();

        page.section(tr("section_motion"));
        page.group_start();
        page.setting(&self.config, AppConfigField::MotionBlur, true);
        page.group_end();
        page
    }

    pub(crate) fn handle_appearance_click(&mut self, input: PageInput) {
        let page = self.build_appearance_page();
        let result = input.hit_test(&page);
        if self.handle_setting_click(&page, &result, input) {
            return;
        }
        let Some(action) = page.action(&result).copied() else {
            return;
        };

        if let (AppearanceAction::RecommendedFont, ClickResult::RowButton(_)) = (action, &result) {
            if let Err(error) =
                crate::platform::shell().open_url(crate::utils::fonts::RECOMMENDED_FONT_URL)
            {
                log::warn!("Could not open font page: {error}");
            }
            return;
        }

        let ClickResult::SourceButton(item_index) = result else {
            return;
        };
        let button_rect = input.popup_button_rect(&page, item_index, self.scroll_y);
        let (win_w, win_h) = self.logical_window_size();
        let popup = match action {
            AppearanceAction::IslandStyle => {
                let host_backdrop = crate::platform::capabilities().host_backdrop;
                let mut labels = vec![tr("style_default")];
                let mut values = vec!["default".to_string()];
                if host_backdrop {
                    labels.push(tr("style_glass"));
                    values.push("glass".to_string());
                }
                labels.push(tr("style_dynamic"));
                values.push("dynamic".to_string());
                labels.push(tr("style_liquid_glass"));
                values.push("liquid".to_string());
                let selected = values
                    .iter()
                    .position(|value| value == &self.config.island_style)
                    .unwrap_or(values.len() - 1);
                PopupState::new(
                    select_island_style,
                    button_rect,
                    labels,
                    values,
                    selected,
                    win_w,
                    win_h,
                )
            }
            AppearanceAction::FontFamily => {
                let families = crate::utils::fonts::family_choices(
                    &FontManager::global().installed_families(),
                );
                let selected = if self.config.custom_font_path.is_some() {
                    1
                } else {
                    self.config
                        .font_family
                        .as_ref()
                        .and_then(|family| families.iter().position(|name| name == family))
                        .map_or(0, |index| index + 2)
                };
                let mut labels = vec![tr("font_system_default"), tr("font_from_file")];
                let mut values = vec![String::new(), FONT_FILE_CHOICE.to_string()];
                labels.extend(families.iter().cloned());
                values.extend(families);
                PopupState::new(
                    select_font_family,
                    button_rect,
                    labels,
                    values,
                    selected,
                    win_w,
                    win_h,
                )
            }
            AppearanceAction::RecommendedFont => return,
        };
        self.show_popup(popup);
    }
}

fn select_island_style(app: &mut SettingsApp, value: &str) {
    app.config.island_style = if value == "glass" && !crate::platform::capabilities().host_backdrop
    {
        "dynamic".to_string()
    } else {
        value.to_string()
    };
}

fn select_font_family(app: &mut SettingsApp, value: &str) {
    match value {
        "" => {
            app.config.font_family = None;
            app.config.custom_font_path = None;
        }
        FONT_FILE_CHOICE => {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Fonts", &["ttf", "otf", "ttc", "otc"])
                .pick_file()
            else {
                return;
            };
            app.config.custom_font_path = Some(path.to_string_lossy().into_owned());
            app.config.font_family = None;
        }
        family => {
            app.config.font_family = Some(family.to_string());
            app.config.custom_font_path = None;
        }
    }
    crate::utils::fonts::apply(&app.config);
}
