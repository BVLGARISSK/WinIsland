use std::time::{Duration, Instant};

use winisland_core::i18n::tr;
use winisland_platform::Key;

use crate::utils::settings_ui::items::SettingsItem;

use super::{BUILTIN_SIDEBAR_PAGE_COUNT, BuiltinPage, SETTINGS_HEADER_H, SettingsApp};

pub(crate) const FLASH_DURATION: Duration = Duration::from_millis(1600);
const MAX_RESULTS: usize = 40;

#[derive(Clone)]
pub(crate) struct SearchResult {
    pub(crate) page: usize,
    pub(crate) item: Option<usize>,
    pub(crate) label: String,
}

#[derive(Default)]
pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) focused: bool,
    pub(crate) composition: String,
    pub(crate) results: Vec<SearchResult>,
    pub(crate) flash: Option<(usize, usize, Instant)>,
}

impl SearchState {
    pub(crate) fn active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    pub(crate) fn flash_active(&self) -> bool {
        self.flash
            .is_some_and(|(_, _, started)| started.elapsed() < FLASH_DURATION)
    }
}

fn item_label(item: &SettingsItem) -> Option<&str> {
    match item {
        SettingsItem::SectionHeader { label }
        | SettingsItem::RowStepper { label, .. }
        | SettingsItem::RowSwitch { label, .. }
        | SettingsItem::RowFolderPicker { label, .. }
        | SettingsItem::RowSourceSelect { label, .. }
        | SettingsItem::RowButton { label, .. } => Some(label),
        _ => None,
    }
}

impl SettingsApp {
    fn searchable_items(&self, page: BuiltinPage) -> Vec<SettingsItem> {
        match page {
            BuiltinPage::General => self.build_general_page().into_items(),
            BuiltinPage::Appearance => self.build_appearance_page().into_items(),
            BuiltinPage::Layout => self.build_layout_page().into_items(),
            BuiltinPage::Behavior => self.build_behavior_page().into_items(),
            BuiltinPage::Alerts => self.build_alerts_page().into_items(),
            BuiltinPage::Music => self.build_music_items(),
            BuiltinPage::LingoSoul => self.build_lingosoul_page(false).into_items(),
            BuiltinPage::Widgets | BuiltinPage::Plugins | BuiltinPage::About => Vec::new(),
        }
    }

    pub(crate) fn refresh_search_results(&mut self) {
        let needle = self.search.query.trim().to_lowercase();
        let mut results = Vec::new();
        if !needle.is_empty() {
            let matches = |text: &str| text.to_lowercase().contains(&needle);
            for page in BuiltinPage::ALL {
                let index = page as usize;
                let title = tr(page.title_key());
                if matches(&title) {
                    results.push(SearchResult {
                        page: index,
                        item: None,
                        label: title,
                    });
                }
                for (item_index, item) in self.searchable_items(page).iter().enumerate() {
                    if let Some(label) = item_label(item).filter(|label| matches(label)) {
                        results.push(SearchResult {
                            page: index,
                            item: Some(item_index),
                            label: label.to_string(),
                        });
                    }
                }
            }
            for (offset, page) in self.plugin_settings_pages.iter().enumerate() {
                if matches(&page.title) {
                    results.push(SearchResult {
                        page: BUILTIN_SIDEBAR_PAGE_COUNT + offset,
                        item: None,
                        label: page.title.clone(),
                    });
                }
            }
        }
        results.truncate(MAX_RESULTS);
        self.search.results = results;
        self.sidebar_hover = -1;
        self.request_redraw();
    }

    pub(crate) fn focus_search(&mut self, focused: bool) {
        if self.search.focused == focused {
            return;
        }
        self.search.focused = focused;
        self.search.composition.clear();
        self.sync_text_input();
        self.request_redraw();
    }

    pub(crate) fn sync_text_input(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let area = self.search.focused.then(|| {
            let scale = self.window_scale();
            let rect = self.search_rect();
            winisland_platform::Rect {
                left: (rect.left * scale) as i32,
                top: (rect.top * scale) as i32,
                right: (rect.right * scale) as i32,
                bottom: (rect.bottom * scale) as i32,
            }
        });
        crate::platform::window().set_text_input(window.id(), area);
    }

    pub(crate) fn set_search_composition(&mut self, text: &str) {
        if self.search.focused && self.search.composition != text {
            self.search.composition = text.to_string();
            self.request_redraw();
        }
    }

    pub(crate) fn clear_search(&mut self) {
        self.search.query.clear();
        self.refresh_search_results();
    }

    pub(crate) fn insert_search_text(&mut self, text: &str) -> bool {
        if !self.search.focused {
            return false;
        }
        self.search.composition.clear();
        let text: String = text
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        if text.is_empty() {
            return true;
        }
        self.search.query.push_str(&text);
        self.refresh_search_results();
        true
    }

    /// Edits the focused search field; returns whether the key was consumed.
    pub(crate) fn handle_search_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        if !self.search.focused {
            return false;
        }
        match key {
            Key::Backspace => {
                self.search.query.pop();
                self.refresh_search_results();
            }
            Key::Escape => {
                if self.search.query.is_empty() {
                    self.focus_search(false);
                } else {
                    self.clear_search();
                }
            }
            Key::Enter => {
                if let Some(result) = self.search.results.first().cloned() {
                    self.open_search_result(&result);
                }
            }
            _ => {
                if let Some(text) = text {
                    self.insert_search_text(text);
                }
            }
        }
        true
    }

    pub(crate) fn open_search_result(&mut self, result: &SearchResult) {
        self.visit_page(result.page);
        let Some(item) = result.item else {
            return;
        };
        self.ensure_items_cache();
        let offset: f32 = self
            .cached_items
            .iter()
            .take(item)
            .map(SettingsItem::height)
            .sum();
        let target = (offset - 12.0).clamp(0.0, self.cached_max_scroll);
        self.scroll_y = target;
        self.target_scroll_y = target;
        self.search.flash = Some((result.page, item, Instant::now()));
        self.request_redraw();
    }

    pub(crate) fn search_flash_y(&self) -> Option<(f32, f32)> {
        let (page, item, _) = self.search.flash?;
        if page != self.active_page || !self.search.flash_active() {
            return None;
        }
        let top = SETTINGS_HEADER_H
            + self
                .cached_items
                .iter()
                .take(item)
                .map(SettingsItem::height)
                .sum::<f32>();
        let height = self.cached_items.get(item).map(SettingsItem::height)?;
        Some((top, height))
    }
}
