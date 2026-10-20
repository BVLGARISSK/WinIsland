use std::collections::HashMap;
use std::time::Instant;

use winisland_core::config::{ExpandedPageKind, PageId, page_order, set_page_order};
use winisland_core::i18n::tr;
use winisland_render::FontWeight;
use winisland_render::text::FontManager;
use winisland_render::{Painter, Path, Point, Radius, Rect, Rgba, StrokeCap, StrokeJoin};

use super::{SETTINGS_HEADER_H, SIDEBAR_W, SettingsApp, WIDGETS_PAGE_INDEX, animate_towards};
use crate::utils::color::SettingsTheme;
use crate::utils::settings_ui::items::{
    CONTENT_PADDING, GROUP_INNER_PAD, GROUP_RADIUS, ROW_HEIGHT, SettingsItem,
};
use crate::utils::settings_ui::{WidgetEditorMode, settings_color};

const SLIDE_RATE: f32 = 16.0;
const LIFT_RATE: f32 = 18.0;
const CHECK_RATE: f32 = 20.0;
const ARROW_BUTTON: f32 = 28.0;
const ARROW_GAP: f32 = 6.0;
const CHECK_SIZE: f32 = 20.0;
const PRESS_SECS: f32 = 0.25;
const HINT_GAP: f32 = 10.0;
const HINT_SIZE: f32 = 12.0;
const HINT_LINE_HEIGHT: f32 = 17.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowDirection {
    Up,
    Down,
}

#[derive(Clone)]
pub(crate) struct PageOrderDrag {
    page: PageId,
    grab_offset: f32,
}

#[derive(Default)]
pub(crate) struct PageOrderState {
    slots: HashMap<PageId, f32>,
    checks: HashMap<PageId, f32>,
    initialized: bool,
    drag: Option<PageOrderDrag>,
    lift: f32,
    press: Option<(PageId, ArrowDirection, Instant)>,
    original_order: Option<Vec<(PageId, bool)>>,
}

pub(crate) fn page_name_key(kind: ExpandedPageKind) -> &'static str {
    match kind {
        ExpandedPageKind::Music => "page_music",
        ExpandedPageKind::Widgets => "page_widgets",
        ExpandedPageKind::Calendar => "page_calendar",
        ExpandedPageKind::Timer => "page_timer",
    }
}

pub(crate) fn page_order_list_height(count: usize) -> f32 {
    count.max(1) as f32 * ROW_HEIGHT
}

fn mix(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Rgba::from_argb(
        channel(from.a(), to.a()),
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

pub(crate) fn draw_checkbox(
    painter: Painter<'_>,
    rect: Rect,
    checked: f32,
    locked: bool,
    theme: &SettingsTheme,
) {
    let accent = settings_color(theme.accent);
    let accent = if locked {
        accent.with_alpha_f(f32::from(accent.a()) / 255.0 * 0.45)
    } else {
        accent
    };
    if checked < 1.0 {
        painter.stroke_round_rect(
            rect.inset(0.75),
            Radius::uniform(5.0),
            1.5,
            settings_color(theme.card_highlight),
        );
    }
    if checked <= 0.001 {
        return;
    }
    let grow = 0.85 + 0.15 * checked;
    let size = rect.width() * grow;
    let fill = Rect::from_xywh(
        rect.center_x() - size / 2.0,
        rect.center_y() - size / 2.0,
        size,
        size,
    );
    painter.fill_round_rect(
        fill,
        Radius::uniform(5.0),
        accent.with_alpha_f(f32::from(accent.a()) / 255.0 * checked),
    );
    let svg = format!(
        "M {} {} L {} {} L {} {}",
        rect.left + 5.0,
        rect.top + 10.0,
        rect.left + 9.0,
        rect.top + 14.0,
        rect.left + 15.0,
        rect.top + 6.0,
    );
    if let Some(path) = Path::from_svg(&svg) {
        painter.stroke_path(
            &path,
            2.0,
            Rgba::WHITE.with_alpha_f(checked),
            StrokeCap::Round,
            StrokeJoin::Round,
        );
    }
}

impl SettingsApp {
    fn active_plugin_pages(&self) -> Vec<winisland_plugin_host::host::PluginPage> {
        let mut pages = self
            .plugin_host
            .as_ref()
            .map(|host| host.pages())
            .unwrap_or_default();
        pages.retain(|page| page.plugin_id != crate::plugin::calendar::ID);
        pages
    }

    /// The stored page order plus loaded plugin pages that have no stored position yet.
    fn full_page_order(&self) -> Vec<(PageId, bool)> {
        let mut pages = page_order(
            &self.config.expanded_page_order,
            &self.config.hidden_expanded_pages,
            &self.config.plugin_pages,
        );
        for page in self.active_plugin_pages() {
            let id = PageId::Plugin {
                plugin_id: page.plugin_id,
                page_key: page.key,
            };
            if !pages.iter().any(|(candidate, _)| *candidate == id) {
                pages.push((id, false));
            }
        }
        pages
    }

    /// The pages listed in settings: built-in pages and pages of loaded plugins.
    fn page_rows(&self) -> Vec<(PageId, bool)> {
        let active = self.active_plugin_pages();
        self.full_page_order()
            .into_iter()
            .filter(|(page, _)| match page {
                PageId::Builtin(_) => true,
                PageId::Plugin {
                    plugin_id,
                    page_key,
                } => active.iter().any(|candidate| {
                    candidate.plugin_id == *plugin_id && candidate.key == *page_key
                }),
            })
            .collect()
    }

    /// Stores a new order of the listed pages; unlisted plugin pages keep their positions.
    fn set_page_rows(&mut self, rows: &[(PageId, bool)]) {
        let listed = self.page_rows();
        let mut rows = rows.iter().cloned();
        let full: Vec<_> = self
            .full_page_order()
            .into_iter()
            .map(|entry| {
                if listed.iter().any(|(page, _)| *page == entry.0) {
                    rows.next().unwrap_or(entry)
                } else {
                    entry
                }
            })
            .collect();
        set_page_order(
            &full,
            &mut self.config.expanded_page_order,
            &mut self.config.hidden_expanded_pages,
            &mut self.config.plugin_pages,
        );
    }

    fn page_name(&self, page: &PageId) -> String {
        match page {
            PageId::Builtin(kind) => tr(page_name_key(*kind)),
            PageId::Plugin {
                plugin_id,
                page_key,
            } => self
                .active_plugin_pages()
                .into_iter()
                .find(|candidate| candidate.plugin_id == *plugin_id && candidate.key == *page_key)
                .map(|candidate| candidate.title)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| page_key.clone()),
        }
    }

    pub(crate) fn page_order_display_height(&self) -> f32 {
        page_order_list_height(self.page_rows().len())
            + HINT_GAP
            + self.page_order_hint_lines().len() as f32 * HINT_LINE_HEIGHT
    }

    fn page_order_hint_lines(&self) -> Vec<String> {
        FontManager::global().wrap_text(
            &tr("page_order_hint"),
            HINT_SIZE,
            winisland_render::FontStyle::normal(),
            self.content_width() - CONTENT_PADDING * 2.0 - GROUP_INNER_PAD * 2.0,
        )
    }

    fn page_order_active(&self) -> bool {
        self.active_page == WIDGETS_PAGE_INDEX
            && self.widget_editor_mode == WidgetEditorMode::Pages
            && !self.resource_editor_open
    }

    fn page_order_list_rect(&self) -> Option<Rect> {
        if !self.page_order_active() {
            return None;
        }
        let mut y = SETTINGS_HEADER_H;
        for item in &self.cached_items {
            if let SettingsItem::Custom { .. } = item {
                return Some(Rect::from_xywh(
                    SIDEBAR_W + CONTENT_PADDING,
                    y - self.scroll_y,
                    self.content_width() - CONTENT_PADDING * 2.0,
                    page_order_list_height(self.page_rows().len()),
                ));
            }
            y += item.height();
        }
        None
    }

    fn page_order_arrow_rect(list: Rect, top: f32, direction: ArrowDirection) -> Rect {
        let top = top + (ROW_HEIGHT - ARROW_BUTTON) / 2.0;
        let right = list.right - GROUP_INNER_PAD;
        let left = match direction {
            ArrowDirection::Down => right - ARROW_BUTTON,
            ArrowDirection::Up => right - ARROW_BUTTON * 2.0 - ARROW_GAP,
        };
        Rect::from_xywh(left, top, ARROW_BUTTON, ARROW_BUTTON)
    }

    fn page_order_check_rect(list: Rect, top: f32) -> Rect {
        Rect::from_xywh(
            list.left + GROUP_INNER_PAD + 26.0,
            top + (ROW_HEIGHT - CHECK_SIZE) / 2.0,
            CHECK_SIZE,
            CHECK_SIZE,
        )
    }

    fn page_order_row_top(&self, list: Rect, slot: usize, page: &PageId) -> f32 {
        let position = if self.page_order.initialized {
            self.page_order
                .slots
                .get(page)
                .copied()
                .unwrap_or(slot as f32)
        } else {
            slot as f32
        };
        list.top + position * ROW_HEIGHT
    }

    fn page_order_row_at(&mut self, x: f32, y: f32) -> Option<(Rect, usize)> {
        if !self.page_order_active() {
            return None;
        }
        self.ensure_items_cache();
        let list = self.page_order_list_rect()?;
        let point = Point::new(x, y);
        if !list.contains(point) || y < SETTINGS_HEADER_H {
            return None;
        }
        self.page_rows()
            .iter()
            .enumerate()
            .rev()
            .find(|(slot, (page, _))| {
                Rect::from_xywh(
                    list.left,
                    self.page_order_row_top(list, *slot, page),
                    list.width(),
                    ROW_HEIGHT,
                )
                .contains(point)
            })
            .map(|(row, _)| (list, row))
    }

    pub(crate) fn page_order_hovered(&mut self, x: f32, y: f32) -> bool {
        self.page_order_active()
            && (self.page_order.drag.is_some() || self.page_order_row_at(x, y).is_some())
    }

    fn move_page(&mut self, from: usize, to: usize) {
        let mut rows = self.page_rows();
        if from == to || from >= rows.len() || to >= rows.len() {
            return;
        }
        let page = rows.remove(from);
        rows.insert(to, page);
        self.set_page_rows(&rows);
    }

    fn page_toggle_locked(rows: &[(PageId, bool)], page: &PageId) -> bool {
        rows.iter()
            .all(|(candidate, hidden)| *hidden || candidate == page)
            && rows
                .iter()
                .any(|(candidate, hidden)| candidate == page && !hidden)
    }

    fn toggle_page_visibility(&mut self, page: &PageId) {
        let mut rows = self.page_rows();
        if Self::page_toggle_locked(&rows, page) {
            return;
        }
        if let Some((_, hidden)) = rows.iter_mut().find(|(candidate, _)| candidate == page) {
            *hidden = !*hidden;
        }
        self.set_page_rows(&rows);
        self.persist_settings_change();
    }

    pub(crate) fn handle_page_order_press(&mut self) -> bool {
        let (x, y) = self.logical_mouse_pos;
        let Some((list, row)) = self.page_order_row_at(x, y) else {
            return false;
        };
        let pointer = Point::new(x, y);
        let rows = self.page_rows();
        let count = rows.len();
        let page = rows[row].0.clone();
        let row_top = self.page_order_row_top(list, row, &page);
        if Self::page_order_check_rect(list, row_top)
            .inset(-6.0)
            .contains(pointer)
        {
            self.toggle_page_visibility(&page);
            return true;
        }
        for direction in [ArrowDirection::Up, ArrowDirection::Down] {
            if !Self::page_order_arrow_rect(list, row_top, direction).contains(pointer) {
                continue;
            }
            let target = match direction {
                ArrowDirection::Up => row.checked_sub(1),
                ArrowDirection::Down => (row + 1 < count).then_some(row + 1),
            };
            if let Some(target) = target {
                self.page_order.press = Some((page.clone(), direction, Instant::now()));
                self.move_page(row, target);
                self.persist_settings_change();
            }
            return true;
        }
        self.page_order.original_order = Some(rows);
        self.page_order.drag = Some(PageOrderDrag {
            page,
            grab_offset: y - row_top,
        });
        self.request_redraw();
        true
    }

    pub(crate) fn update_page_order_drag(&mut self) -> bool {
        let Some(drag) = self.page_order.drag.clone() else {
            return false;
        };
        self.ensure_items_cache();
        let Some(list) = self.page_order_list_rect() else {
            return false;
        };
        let rows = self.page_rows();
        let count = rows.len();
        let pointer_slot = (self.logical_mouse_pos.1 - drag.grab_offset - list.top) / ROW_HEIGHT;
        let target = pointer_slot
            .round()
            .clamp(0.0, count.saturating_sub(1) as f32) as usize;
        if let Some(current) = rows.iter().position(|(page, _)| *page == drag.page)
            && current != target
        {
            self.move_page(current, target);
            self.mark_items_dirty();
        }
        true
    }

    pub(crate) fn handle_page_order_release(&mut self) -> bool {
        if self.page_order.drag.is_none() {
            return false;
        }
        self.update_page_order_drag();
        self.page_order.drag = None;
        let changed = self
            .page_order
            .original_order
            .take()
            .is_some_and(|original| original != self.page_rows());
        if changed {
            self.persist_settings_change();
        } else {
            self.mark_items_dirty();
        }
        true
    }

    pub(crate) fn cancel_page_order_drag(&mut self) -> bool {
        if self.page_order.drag.take().is_none() {
            return false;
        }
        if let Some(original) = self.page_order.original_order.take() {
            self.set_page_rows(&original);
        }
        self.mark_items_dirty();
        self.request_redraw();
        true
    }

    pub(crate) fn page_order_animating(&self) -> bool {
        if !self.page_order_active() {
            return false;
        }
        let state = &self.page_order;
        let lift_target = f32::from(state.drag.is_some());
        (state.lift - lift_target).abs() > 0.001
            || state.drag.is_some()
            || state
                .press
                .as_ref()
                .is_some_and(|(_, _, started)| started.elapsed().as_secs_f32() < PRESS_SECS)
            || self
                .page_rows()
                .iter()
                .enumerate()
                .any(|(slot, (page, hidden))| {
                    state
                        .slots
                        .get(page)
                        .is_none_or(|value| (value - slot as f32).abs() > 0.001)
                        || state
                            .checks
                            .get(page)
                            .is_none_or(|value| (value - f32::from(!hidden)).abs() > 0.001)
                })
    }

    pub(crate) fn update_page_order_animation(&mut self, dt: f32) -> bool {
        if !self.page_order_active() {
            self.page_order.initialized = false;
            return false;
        }
        let rows = self.page_rows();
        let dragged = self.page_order.drag.as_ref().map(|drag| drag.page.clone());
        let state = &mut self.page_order;
        let mut changed = false;
        state
            .slots
            .retain(|page, _| rows.iter().any(|(candidate, _)| candidate == page));
        state
            .checks
            .retain(|page, _| rows.iter().any(|(candidate, _)| candidate == page));
        for (slot, (page, hidden)) in rows.iter().enumerate() {
            let check_target = f32::from(!hidden);
            if !state.initialized || !state.slots.contains_key(page) {
                state.slots.insert(page.clone(), slot as f32);
                state.checks.insert(page.clone(), check_target);
                changed = true;
                continue;
            }
            if dragged.as_ref() != Some(page)
                && let Some(value) = state.slots.get_mut(page)
            {
                changed |= animate_towards(value, slot as f32, SLIDE_RATE, dt);
            }
            if let Some(value) = state.checks.get_mut(page) {
                changed |= animate_towards(value, check_target, CHECK_RATE, dt);
            }
        }
        state.initialized = true;
        changed |= animate_towards(&mut state.lift, f32::from(dragged.is_some()), LIFT_RATE, dt);
        if state
            .press
            .as_ref()
            .is_some_and(|(_, _, started)| started.elapsed().as_secs_f32() >= PRESS_SECS)
        {
            state.press = None;
            changed = true;
        }
        changed || state.press.is_some()
    }

    pub(crate) fn draw_page_order_list(&self, painter: Painter<'_>, theme: &SettingsTheme) {
        let Some(list) = self.page_order_list_rect() else {
            return;
        };
        let (window_width, window_height) = self.logical_window_size();
        let save_count = painter.save();
        painter.clip_rect(Rect::from_xywh(
            SIDEBAR_W,
            SETTINGS_HEADER_H,
            window_width - SIDEBAR_W,
            window_height - SETTINGS_HEADER_H,
        ));
        painter.fill_round_rect(
            list.offset(winisland_render::Vec2::new(0.0, 2.0)),
            Radius::uniform(GROUP_RADIUS),
            settings_color(theme.shadow),
        );
        painter.fill_round_rect(
            list,
            Radius::uniform(GROUP_RADIUS),
            settings_color(theme.group_bg),
        );
        painter.stroke_round_rect(
            list.inset(0.375),
            Radius::uniform(GROUP_RADIUS),
            0.75,
            settings_color(theme.group_border),
        );

        let order = self.page_rows();
        let count = order.len();
        for slot in 1..count {
            painter.fill_rect(
                Rect::from_xywh(
                    list.left + GROUP_INNER_PAD,
                    list.top + slot as f32 * ROW_HEIGHT,
                    list.width() - GROUP_INNER_PAD * 2.0,
                    0.75,
                ),
                settings_color(theme.separator),
            );
        }

        let dragged = self.page_order.drag.as_ref();
        let pointer = Point::new(self.logical_mouse_pos.0, self.logical_mouse_pos.1);
        let mut rows: Vec<(usize, PageId, bool, f32)> = order
            .iter()
            .enumerate()
            .map(|(slot, (page, hidden))| {
                let top = match dragged {
                    Some(drag) if drag.page == *page => pointer.y - drag.grab_offset,
                    _ => self.page_order_row_top(list, slot, page),
                };
                (slot, page.clone(), *hidden, top)
            })
            .collect();
        rows.sort_by_key(|(_, page, _, _)| dragged.is_some_and(|drag| drag.page == *page));

        for (slot, kind, hidden, top) in rows {
            let is_dragged = dragged.is_some_and(|drag| drag.page == kind);
            let lift = if is_dragged {
                self.page_order.lift
            } else {
                0.0
            };
            let row = Rect::from_xywh(list.left, top, list.width(), ROW_HEIGHT);
            if lift > 0.001 {
                let grow = 3.0 * lift;
                let card = Rect::from_xywh(
                    row.left - grow,
                    row.top - grow,
                    row.width() + grow * 2.0,
                    row.height() + grow * 2.0,
                );
                painter.fill_round_rect(
                    card.offset(winisland_render::Vec2::new(0.0, 4.0 * lift)),
                    Radius::uniform(GROUP_RADIUS),
                    Rgba::from_argb((70.0 * lift) as u8, 0, 0, 0),
                );
                painter.fill_round_rect(
                    card,
                    Radius::uniform(GROUP_RADIUS),
                    settings_color(theme.card_highlight),
                );
            }

            let handle_x = row.left + GROUP_INNER_PAD;
            for line in 0..3 {
                painter.fill_round_rect(
                    Rect::from_xywh(
                        handle_x,
                        row.center_y() - 5.0 + line as f32 * 4.5,
                        14.0,
                        1.6,
                    ),
                    Radius::uniform(0.8),
                    settings_color(theme.text_sec).with_alpha(150),
                );
            }

            let checked = self
                .page_order
                .checks
                .get(&kind)
                .copied()
                .filter(|_| self.page_order.initialized)
                .unwrap_or(f32::from(!hidden));
            let check = Self::page_order_check_rect(list, top);
            draw_checkbox(
                painter,
                check,
                checked,
                Self::page_toggle_locked(&order, &kind),
                theme,
            );

            let fonts = FontManager::global();
            let name = self.page_name(&kind);
            let name_bounds = fonts.measure_str(&name, 14.0, FontWeight::REGULAR);
            let text_x = check.right + 12.0;
            let baseline = row.center_y() - name_bounds.height() / 2.0 - name_bounds.top;
            let primary = settings_color(theme.text_pri);
            let secondary = settings_color(theme.text_sec);
            let name_color = mix(secondary, primary, checked);
            fonts.draw_str(
                painter,
                &name,
                Point::new(text_x - name_bounds.left, baseline),
                14.0,
                FontWeight::REGULAR,
                name_color,
            );
            let position = (slot + 1).to_string();
            fonts.draw_str(
                painter,
                &position,
                Point::new(text_x + name_bounds.width() + 10.0, baseline),
                12.0,
                FontWeight::REGULAR,
                secondary,
            );

            if is_dragged {
                continue;
            }
            for direction in [ArrowDirection::Up, ArrowDirection::Down] {
                let button = Self::page_order_arrow_rect(list, top, direction);
                let enabled = match direction {
                    ArrowDirection::Up => slot > 0,
                    ArrowDirection::Down => slot + 1 < count,
                };
                let hovered = enabled && dragged.is_none() && button.contains(pointer);
                let flash = self
                    .page_order
                    .press
                    .as_ref()
                    .filter(|(pressed, pressed_direction, _)| {
                        *pressed == kind && *pressed_direction == direction
                    })
                    .map_or(0.0, |(_, _, started)| {
                        1.0 - (started.elapsed().as_secs_f32() / PRESS_SECS).min(1.0)
                    });
                let fill = if hovered || flash > 0.0 {
                    theme.control_hover
                } else {
                    theme.control_bg
                };
                let shrink = 1.5 * flash;
                painter.fill_round_rect(
                    button.inset(shrink),
                    Radius::uniform(7.0),
                    settings_color(fill),
                );
                let color = settings_color(if enabled {
                    theme.text_pri
                } else {
                    theme.disabled
                });
                let center = Point::new(button.center_x(), button.center_y());
                let sign = if direction == ArrowDirection::Up {
                    -1.0
                } else {
                    1.0
                };
                for side in [-1.0, 1.0] {
                    painter.stroke_line(
                        Point::new(center.x + side * 4.5, center.y - sign * 2.2),
                        Point::new(center.x, center.y + sign * 2.4),
                        1.7,
                        color,
                        StrokeCap::Round,
                    );
                }
            }
        }
        let fonts = FontManager::global();
        for (index, line) in self.page_order_hint_lines().iter().enumerate() {
            fonts.draw_str(
                painter,
                line,
                Point::new(
                    list.left + GROUP_INNER_PAD,
                    list.bottom + HINT_GAP + HINT_SIZE + index as f32 * HINT_LINE_HEIGHT,
                ),
                HINT_SIZE,
                FontWeight::REGULAR,
                settings_color(theme.text_sec),
            );
        }
        painter.restore_to(save_count);
    }
}
