use std::time::{Duration, Instant};

use winisland_render::Rect;

use crate::utils::settings_ui::items::{POPUP_ITEM_H, POPUP_MENU_PAD};
use winisland_render::text::FontManager;

use super::SettingsApp;

pub(crate) type PopupSelectHandler = fn(&mut SettingsApp, &str);

const POPUP_MAX_VISIBLE_ITEMS: usize = 12;
const POPUP_MAX_TEXT_WIDTH: f32 = 260.0;
const TYPE_AHEAD_RESET: Duration = Duration::from_millis(900);
const POPUP_SCROLL_GUTTER: f32 = 12.0;
const POPUP_THUMB_WIDTH: f32 = 4.0;
const POPUP_THUMB_MIN: f32 = 24.0;
const POPUP_THUMB_INSET: f32 = 4.0;

pub(crate) struct PopupState {
    pub(crate) on_select: PopupSelectHandler,
    pub(crate) button_rect: Rect,
    pub(crate) menu_rect: Rect,
    pub(crate) options: Vec<String>,
    pub(crate) values: Vec<String>,
    pub(crate) selected_idx: usize,
    pub(crate) hover_idx: Option<usize>,
    pub(crate) scroll: f32,
    pub(crate) keyboard_anchor: Option<(f32, f32)>,
    pub(crate) scroll_drag: Option<f32>,
    typed: String,
    typed_at: Option<Instant>,
}

impl PopupState {
    pub(crate) fn new(
        on_select: PopupSelectHandler,
        button_rect: Rect,
        options: Vec<String>,
        values: Vec<String>,
        selected_idx: usize,
        window_width: f32,
        window_height: f32,
    ) -> Self {
        let font_manager = FontManager::global();
        let content_width = options
            .iter()
            .map(|option| {
                font_manager.measure_text_cached(
                    option,
                    13.0,
                    winisland_render::FontStyle::normal(),
                )
            })
            .fold(120.0_f32, f32::max)
            .min(POPUP_MAX_TEXT_WIDTH);
        let max_items_height = (window_height - 20.0 - POPUP_MENU_PAD * 2.0).max(POPUP_ITEM_H);
        let visible_items = options
            .len()
            .min(POPUP_MAX_VISIBLE_ITEMS)
            .min((max_items_height / POPUP_ITEM_H).floor().max(1.0) as usize);
        let gutter = if options.len() > visible_items {
            POPUP_SCROLL_GUTTER
        } else {
            0.0
        };
        let menu_width = content_width + 36.0 + gutter;
        let menu_height = visible_items as f32 * POPUP_ITEM_H + POPUP_MENU_PAD * 2.0;
        let max_x = (window_width - menu_width - 10.0).max(0.0);
        let max_y = (window_height - menu_height - 10.0).max(0.0);
        let menu_x = (button_rect.right - menu_width).clamp(0.0, max_x);
        let below_y = button_rect.bottom + 4.0;
        let above_y = button_rect.top - menu_height - 4.0;
        let opens_upward = below_y + menu_height > window_height - 10.0 && above_y >= 10.0;
        let menu_y = if opens_upward {
            above_y
        } else {
            below_y.min(max_y)
        };

        let mut popup = Self {
            on_select,
            button_rect,
            menu_rect: Rect::from_xywh(menu_x, menu_y, menu_width, menu_height),
            options,
            values,
            selected_idx,
            hover_idx: None,
            scroll: 0.0,
            keyboard_anchor: None,
            scroll_drag: None,
            typed: String::new(),
            typed_at: None,
        };
        let centered = (selected_idx as f32 + 0.5) * POPUP_ITEM_H - popup.visible_height() / 2.0;
        popup.scroll_by(centered);
        popup
    }

    pub(crate) fn menu_rect(&self) -> Rect {
        self.menu_rect
    }

    pub(crate) fn inner_rect(&self) -> Rect {
        Rect::from_ltrb(
            self.menu_rect.left,
            self.menu_rect.top + POPUP_MENU_PAD,
            self.menu_rect.right,
            self.menu_rect.bottom - POPUP_MENU_PAD,
        )
    }

    fn visible_height(&self) -> f32 {
        self.menu_rect.height() - POPUP_MENU_PAD * 2.0
    }

    pub(crate) fn max_scroll(&self) -> f32 {
        (self.options.len() as f32 * POPUP_ITEM_H - self.visible_height()).max(0.0)
    }

    pub(crate) fn scroll_by(&mut self, delta: f32) -> bool {
        let scroll = (self.scroll + delta).clamp(0.0, self.max_scroll());
        let changed = (scroll - self.scroll).abs() > f32::EPSILON;
        self.scroll = scroll;
        changed
    }

    pub(crate) fn type_ahead(&mut self, text: &str) -> bool {
        let now = Instant::now();
        if self
            .typed_at
            .is_none_or(|typed_at| now.duration_since(typed_at) > TYPE_AHEAD_RESET)
        {
            self.typed.clear();
        }
        self.typed_at = Some(now);
        self.typed.push_str(&text.to_lowercase());
        let Some(index) = self
            .options
            .iter()
            .position(|option| option.to_lowercase().starts_with(&self.typed))
        else {
            return false;
        };
        self.highlight(index);
        true
    }

    pub(crate) fn move_highlight(&mut self, step: isize) {
        let last = self.options.len().saturating_sub(1);
        let index = match self.hover_idx {
            Some(index) => index.saturating_add_signed(step).min(last),
            None if step < 0 => last,
            None => 0,
        };
        self.highlight(index);
    }

    fn highlight(&mut self, index: usize) {
        self.hover_idx = Some(index);
        let top = index as f32 * POPUP_ITEM_H;
        let bottom = top + POPUP_ITEM_H;
        if top < self.scroll {
            self.scroll_by(top - self.scroll);
        } else if bottom > self.scroll + self.visible_height() {
            self.scroll_by(bottom - self.scroll - self.visible_height());
        }
    }

    fn gutter(&self) -> f32 {
        if self.max_scroll() > 0.0 {
            POPUP_SCROLL_GUTTER
        } else {
            0.0
        }
    }

    pub(crate) fn item_rect(&self, index: usize) -> Rect {
        let y = self.menu_rect.top + POPUP_MENU_PAD + index as f32 * POPUP_ITEM_H - self.scroll;
        Rect::from_xywh(
            self.menu_rect.left + POPUP_MENU_PAD,
            y,
            self.menu_rect.width() - POPUP_MENU_PAD * 2.0 - self.gutter(),
            POPUP_ITEM_H,
        )
    }

    fn scroll_track(&self) -> Option<Rect> {
        (self.max_scroll() > 0.0).then(|| {
            let inner = self.inner_rect();
            Rect::from_ltrb(
                self.menu_rect.right - POPUP_MENU_PAD - POPUP_SCROLL_GUTTER,
                inner.top,
                self.menu_rect.right,
                inner.bottom,
            )
        })
    }

    pub(crate) fn thumb_rect(&self) -> Option<Rect> {
        let track = self.scroll_track()?;
        let max_scroll = self.max_scroll();
        let travel_h = track.height() - POPUP_THUMB_INSET * 2.0;
        let visible = self.visible_height();
        let height = (travel_h * visible / (visible + max_scroll))
            .max(POPUP_THUMB_MIN)
            .min(travel_h);
        let center_x = self.menu_rect.right - POPUP_MENU_PAD - POPUP_SCROLL_GUTTER / 2.0;
        Some(Rect::from_xywh(
            center_x - POPUP_THUMB_WIDTH / 2.0,
            track.top + POPUP_THUMB_INSET + (travel_h - height) * self.scroll / max_scroll,
            POPUP_THUMB_WIDTH,
            height,
        ))
    }

    pub(crate) fn scrollbar_hovered(&self, mouse_x: f32, mouse_y: f32) -> bool {
        self.scroll_track()
            .is_some_and(|track| track.contains(winisland_render::Point::new(mouse_x, mouse_y)))
    }

    /// Grabs the scroll thumb, or jumps it under the pointer when the track is pressed.
    pub(crate) fn begin_scroll_drag(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        let Some(thumb) = self
            .thumb_rect()
            .filter(|_| self.scrollbar_hovered(mouse_x, mouse_y))
        else {
            return false;
        };
        self.scroll_drag = Some(if mouse_y >= thumb.top && mouse_y <= thumb.bottom {
            mouse_y - thumb.top
        } else {
            thumb.height() / 2.0
        });
        self.drag_scroll(mouse_y);
        true
    }

    pub(crate) fn drag_scroll(&mut self, mouse_y: f32) -> bool {
        let (Some(grab), Some(track), Some(thumb)) =
            (self.scroll_drag, self.scroll_track(), self.thumb_rect())
        else {
            return false;
        };
        let travel = track.height() - POPUP_THUMB_INSET * 2.0 - thumb.height();
        if travel <= 0.0 {
            return false;
        }
        let ratio = ((mouse_y - grab - track.top - POPUP_THUMB_INSET) / travel).clamp(0.0, 1.0);
        let scroll = ratio * self.max_scroll();
        let changed = (scroll - self.scroll).abs() > f32::EPSILON;
        self.scroll = scroll;
        changed
    }

    pub(crate) fn end_scroll_drag(&mut self) -> bool {
        self.scroll_drag.take().is_some()
    }

    pub(crate) fn visible_items(&self) -> std::ops::Range<usize> {
        let first = (self.scroll / POPUP_ITEM_H).floor() as usize;
        let last = ((self.scroll + self.visible_height()) / POPUP_ITEM_H).ceil() as usize;
        first.min(self.options.len())..last.min(self.options.len())
    }

    pub(crate) fn hit_test_item(&self, mouse_x: f32, mouse_y: f32) -> Option<usize> {
        let inner = self.inner_rect();
        if mouse_x < inner.left
            || mouse_x > inner.right - self.gutter()
            || mouse_y < inner.top
            || mouse_y > inner.bottom
        {
            return None;
        }

        let index = ((mouse_y - inner.top + self.scroll) / POPUP_ITEM_H).floor() as usize;
        (index < self.options.len()).then_some(index)
    }
}
