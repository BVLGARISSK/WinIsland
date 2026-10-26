use crate::utils::settings_ui::WidgetEditorMode;
use crate::utils::settings_ui::hover_test;
use winisland_platform::Key;
use winisland_render::{Point, Rect};

use super::pages::PageInput;
use super::sidebar::SidebarEntry;
use super::{
    BuiltinPage, MUSIC_PAGE_INDEX, NumberInput, NumberInputHandler, PAGE_NAV_GAP, PAGE_NAV_HEIGHT,
    PAGE_NAV_WIDTH, PAGE_NAV_X, PAGE_NAV_Y, PLUGINS_PAGE_INDEX, POPUP_OPACITY_KEY, PageNavigation,
    SETTINGS_HEADER_H, SIDEBAR_W, SettingsApp,
};

impl SettingsApp {
    pub(crate) fn music_notice_button_hovered(&self) -> bool {
        if self.active_page != MUSIC_PAGE_INDEX || !self.show_music_notice() {
            return false;
        }
        let (x, y) = self.logical_mouse_pos;
        super::pages::music::music_notice_button_rect(self.content_width(), SETTINGS_HEADER_H)
            .contains(Point::new(x - SIDEBAR_W, y + self.scroll_y))
    }

    pub(super) fn close_popup(&mut self, index: Option<usize>) {
        let plugin_setting = self.pending_plugin_setting.is_some();
        let selection = self.popup.take().and_then(|popup| {
            index
                .and_then(|index| popup.values.get(index))
                .map(|value| (popup.on_select, value.clone()))
        });
        self.anim.set_with_speed(POPUP_OPACITY_KEY, 0.0, 0.3);
        if let Some((on_select, value)) = selection {
            on_select(self, &value);
            if !plugin_setting {
                self.persist_settings_change();
            }
        } else {
            self.pending_plugin_setting = None;
            self.request_redraw();
        }
    }

    pub(super) fn handle_popup_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        let Some(popup) = self.popup.as_mut() else {
            return false;
        };
        popup.keyboard_anchor = Some(self.logical_mouse_pos);
        match key {
            Key::Escape => self.close_popup(None),
            Key::Enter => {
                let index = popup.hover_idx;
                self.close_popup(index);
            }
            Key::ArrowDown => {
                popup.move_highlight(1);
                self.request_redraw();
            }
            Key::ArrowUp => {
                popup.move_highlight(-1);
                self.request_redraw();
            }
            _ => {
                if let Some(text) = text.filter(|text| !text.chars().any(char::is_control))
                    && popup.type_ahead(text)
                {
                    self.request_redraw();
                }
            }
        }
        true
    }

    pub(super) fn handle_click(&mut self) {
        let (mouse_x, mouse_y) = self.logical_mouse_pos;

        self.commit_number_input();

        if let Some(popup) = &self.popup {
            let index = popup.hit_test_item(mouse_x, mouse_y);
            self.close_popup(index);
            return;
        }

        if self.resource_editor_open {
            self.handle_resource_editor_click(mouse_x, mouse_y);
            return;
        }

        if mouse_x < SIDEBAR_W {
            self.handle_sidebar_click(mouse_x, mouse_y);
            return;
        }
        self.focus_search(false);

        if self.handle_widget_mode_click(mouse_x, mouse_y) {
            return;
        }

        if let Some(direction) = Self::page_navigation_at(mouse_x, mouse_y) {
            self.navigate_page_history(direction);
            return;
        }

        let content_width = self.content_width();
        let input = PageInput {
            x: mouse_x - SIDEBAR_W,
            y: mouse_y + self.scroll_y,
            width: content_width,
            start_y: SETTINGS_HEADER_H,
        };

        match BuiltinPage::from_index(self.active_page) {
            Some(BuiltinPage::General) => self.handle_general_click(input),
            Some(BuiltinPage::Appearance) => self.handle_appearance_click(input),
            Some(BuiltinPage::Layout) => self.handle_layout_click(input),
            Some(BuiltinPage::Behavior) => self.handle_behavior_click(input),
            Some(BuiltinPage::Alerts) => self.handle_alerts_click(input),
            Some(BuiltinPage::Music) => self.handle_music_click(input),
            Some(BuiltinPage::Widgets) if self.widget_editor_mode == WidgetEditorMode::Pages => {}
            Some(BuiltinPage::Widgets) => {
                if self.handle_widget_click()
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            Some(BuiltinPage::Plugins) => self.handle_plugin_click(),
            Some(BuiltinPage::LingoSoul) => self.handle_lingosoul_click(input),
            Some(BuiltinPage::About) => self.handle_about_click(input),
            None => self.handle_plugin_settings_click(input),
        }
    }

    fn reset_scroll(&mut self) {
        self.scroll_y = 0.0;
        self.target_scroll_y = 0.0;
        self.widget_hover_target = None;
        self.widget_hover_visual = None;
        self.widget_hover_progress = 0.0;
        self.widget_preview_hover_slot = None;
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(super) fn get_hover_state(&mut self) -> bool {
        let (mouse_x, mouse_y) = self.logical_mouse_pos;
        if self.resource_editor_open {
            if let Some(popup) = &self.popup
                && popup.menu_rect().contains(Point::new(mouse_x, mouse_y))
            {
                return true;
            }
            return self.resource_editor_control_at(mouse_x, mouse_y);
        }
        if self.scroll_dragging
            || self
                .scrollbar_geometry()
                .is_some_and(|scrollbar| scrollbar.hit_test(mouse_x, mouse_y))
        {
            return true;
        }
        if let Some(direction) = Self::page_navigation_at(mouse_x, mouse_y) {
            let is_enabled = match direction {
                PageNavigation::Back => self.can_navigate_back(),
                PageNavigation::Forward => self.can_navigate_forward(),
            };
            if is_enabled {
                return true;
            }
        }
        if self.widget_mode_at(mouse_x, mouse_y).is_some()
            || self.page_order_hovered(mouse_x, mouse_y)
        {
            return true;
        }

        if let Some(popup) = &self.popup {
            let menu = popup.menu_rect();
            if mouse_x >= menu.left
                && mouse_x <= menu.right
                && mouse_y >= menu.top
                && mouse_y <= menu.bottom
            {
                return true;
            }
        }

        if mouse_x < SIDEBAR_W {
            return self.sidebar_entry_at(mouse_x, mouse_y).is_some()
                || self.search_rect().contains(Point::new(mouse_x, mouse_y));
        }

        let content_width = self.content_width();
        if self.music_notice_button_hovered() {
            return true;
        }
        if self.widget_drag_active() {
            return true;
        }
        if self.active_page == PLUGINS_PAGE_INDEX {
            return self.plugin_hovered();
        }
        if self.widget_preview_hovered_at_mouse() {
            return true;
        }

        self.ensure_items_cache();
        hover_test(
            &self.cached_items,
            mouse_x - SIDEBAR_W,
            mouse_y + self.scroll_y,
            SETTINGS_HEADER_H,
            content_width,
        )
    }

    fn handle_sidebar_click(&mut self, x: f32, y: f32) {
        let point = Point::new(x, y);
        let search = self.search_rect();
        if search.contains(point) {
            let clear = crate::utils::settings_ui::chrome::search_clear_center(search);
            if self.search.active() && (x - clear.x).abs() <= 9.0 {
                self.clear_search();
            }
            self.focus_search(true);
            return;
        }
        self.focus_search(false);
        match self.sidebar_entry_at(x, y) {
            Some(SidebarEntry::Page(page)) => self.visit_page(page),
            Some(SidebarEntry::Result(index)) => {
                if let Some(result) = self.search.results.get(index).cloned() {
                    self.open_search_result(&result);
                }
            }
            None => {}
        }
    }

    pub(super) fn page_navigation_at(mouse_x: f32, mouse_y: f32) -> Option<PageNavigation> {
        if !(PAGE_NAV_Y..=PAGE_NAV_Y + PAGE_NAV_HEIGHT).contains(&mouse_y) {
            return None;
        }

        if (PAGE_NAV_X..=PAGE_NAV_X + PAGE_NAV_WIDTH).contains(&mouse_x) {
            return Some(PageNavigation::Back);
        }

        let forward_x = PAGE_NAV_X + PAGE_NAV_WIDTH + PAGE_NAV_GAP;
        if (forward_x..=forward_x + PAGE_NAV_WIDTH).contains(&mouse_x) {
            return Some(PageNavigation::Forward);
        }

        None
    }

    pub(crate) fn can_navigate_back(&self) -> bool {
        self.page_history_index > 0
    }

    pub(crate) fn can_navigate_forward(&self) -> bool {
        self.page_history_index + 1 < self.page_history.len()
    }

    pub(super) fn navigate_page_history(&mut self, direction: PageNavigation) -> bool {
        let next_index = match direction {
            PageNavigation::Back if self.can_navigate_back() => self.page_history_index - 1,
            PageNavigation::Forward if self.can_navigate_forward() => self.page_history_index + 1,
            _ => return false,
        };
        self.page_history_index = next_index;
        self.cancel_page_order_drag();
        self.active_page = self.page_history[next_index];
        if self.active_page != PLUGINS_PAGE_INDEX {
            self.selected_plugin_id = None;
            self.plugin_detail_closing = false;
            self.anim
                .set_with_speed(super::PLUGIN_DETAIL_KEY, 0.0, 0.28);
        }
        self.reset_scroll();
        true
    }

    pub(crate) fn visit_page(&mut self, page: usize) {
        if self.active_page == page {
            return;
        }
        self.cancel_page_order_drag();
        self.page_history.truncate(self.page_history_index + 1);
        self.page_history.push(page);
        self.page_history_index = self.page_history.len() - 1;
        self.active_page = page;
        if page != PLUGINS_PAGE_INDEX {
            self.selected_plugin_id = None;
            self.plugin_detail_closing = false;
            self.anim
                .set_with_speed(super::PLUGIN_DETAIL_KEY, 0.0, 0.28);
        }
        self.reset_scroll();
    }

    pub(crate) fn begin_number_input(
        &mut self,
        rect: Rect,
        value: String,
        on_commit: NumberInputHandler,
    ) {
        self.number_input = Some(NumberInput {
            rect,
            text: value,
            on_commit,
        });
        self.request_redraw();
    }

    pub(super) fn commit_number_input(&mut self) {
        let Some(input) = self.number_input.take() else {
            return;
        };
        let plugin_setting = self.pending_plugin_setting.is_some();
        (input.on_commit)(self, &input.text);
        if !plugin_setting {
            self.persist_settings_change();
        }
    }

    pub(super) fn handle_number_input_key(&mut self, key: &Key) -> bool {
        let Some(input) = &mut self.number_input else {
            return false;
        };

        match key {
            Key::Backspace => {
                input.text.pop();
            }
            Key::Enter => {
                self.commit_number_input();
                return true;
            }
            Key::Escape => {
                self.number_input = None;
                self.pending_plugin_setting = None;
            }
            Key::Character(value)
                if value.chars().all(|character| {
                    character.is_ascii_digit() || matches!(character, '.' | '-')
                }) =>
            {
                input.text.push_str(value);
            }
            _ => return false,
        }

        self.request_redraw();
        true
    }
}
