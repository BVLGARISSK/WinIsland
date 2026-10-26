use std::cell::LazyCell;
use winisland_core::i18n::tr;
use winisland_render::text::{DrawTextCachedParams, FontManager};

use winisland_render::{
    FontStyle, FontWeight, Image, ImageOptions, Painter, Point, Radius, Rect, Rgba, Sampling, Vec2,
};

use crate::core::assistant::ChatMessage;
use crate::ui::markdown::{self, MarkdownColors, MarkdownRenderParams};

const PAD: f32 = 14.0;
const TOP_PAD: f32 = 10.0;
const HEADER_H: f32 = 32.0;
/// Corner radius of the open panel, in logical pixels.
pub(crate) const PANEL_RADIUS: f32 = 30.0;
const INPUT_H: f32 = 38.0;
const INPUT_GAP: f32 = 10.0;
const MESSAGE_GAP: f32 = 14.0;
const BUBBLE_PAD_X: f32 = 12.0;
const BUBBLE_PAD_Y: f32 = 8.0;
const TEXT_SIZE: f32 = 12.0;
const LINE_HEIGHT: f32 = 18.0;
const TITLE_SIZE: f32 = 15.0;
const BUTTON_SIZE: f32 = 12.0;
const USER_MAX_WIDTH: f32 = 0.8;
const THINKING_H: f32 = 20.0;
const WHEEL_STEP: f32 = 48.0;

const TEXT: Rgba = Rgba::WHITE.with_alpha(240);
const SECONDARY: Rgba = Rgba::WHITE.with_alpha(150);
const USER_BUBBLE: Rgba = Rgba::from_rgba(52, 120, 246, 230);
const FIELD: Rgba = Rgba::WHITE.with_alpha(24);
const FIELD_FOCUSED: Rgba = Rgba::WHITE.with_alpha(36);
const BUTTON: Rgba = Rgba::WHITE.with_alpha(26);
const LISTENING: Rgba = Rgba::from_rgb(255, 69, 58);
const MIC_GAP: f32 = 4.0;

thread_local! {
    static MIC_ICON: LazyCell<Option<(Image, f32)>> = LazyCell::new(|| {
        crate::icons::load_tinted_glyph(
            include_bytes!("../../resources/in_app/icons/microphone.fill.png"),
            Rgba::WHITE,
        )
    });
    static STOP_ICON: LazyCell<Option<(Image, f32)>> = LazyCell::new(|| {
        crate::icons::load_tinted_glyph(
            include_bytes!("../../resources/in_app/icons/stop.fill.png"),
            Rgba::WHITE,
        )
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChatHit {
    Close,
    Clear,
    Input,
    Mic,
    Panel,
}

struct Layout {
    width: f32,
    content_w: f32,
    area_top: f32,
    area_bottom: f32,
    input: Rect,
    mic: Option<Rect>,
    close: Rect,
    clear: Rect,
}

fn layout(width: f32, height: f32, voice: bool) -> Layout {
    let close_w = FontManager::global().measure_text_cached(
        &tr("lingosoul_chat_close"),
        BUTTON_SIZE,
        FontWeight::MEDIUM,
    ) + 20.0;
    let clear_w = FontManager::global().measure_text_cached(
        &tr("lingosoul_chat_clear"),
        BUTTON_SIZE,
        FontWeight::MEDIUM,
    ) + 20.0;
    let button_h = 24.0;
    let button_y = TOP_PAD + (HEADER_H - 6.0 - button_h) * 0.5;
    let close = Rect::from_xywh(width - PAD - close_w, button_y, close_w, button_h);
    let clear = Rect::from_xywh(close.left - 8.0 - clear_w, button_y, clear_w, button_h);
    let input = Rect::from_xywh(PAD, height - PAD - INPUT_H, width - PAD * 2.0, INPUT_H);
    let mic = voice.then(|| {
        let size = INPUT_H - MIC_GAP * 2.0;
        Rect::from_xywh(
            input.right - MIC_GAP - size,
            input.top + MIC_GAP,
            size,
            size,
        )
    });
    Layout {
        width,
        content_w: (width - PAD * 2.0).max(1.0),
        area_top: TOP_PAD + HEADER_H,
        area_bottom: input.top - INPUT_GAP,
        input,
        mic,
        close,
        clear,
    }
}

/// The chat panel the reply card opens into: history, scrolling and a text field.
#[derive(Default)]
pub(crate) struct ChatPanel {
    messages: Vec<ChatMessage>,
    revision: Option<u64>,
    heights: Vec<f32>,
    heights_width: f32,
    /// Distance scrolled up from the newest message, in logical pixels.
    scroll: f32,
    input: String,
    caret: usize,
    preedit: String,
    pub(crate) focused: bool,
    pub(crate) thinking: bool,
    pub(crate) connected: bool,
    /// The assistant has a microphone, so the field offers voice input.
    pub(crate) voice: bool,
    pub(crate) listening: bool,
    pub(crate) level: f32,
    area_height: f32,
    size: (f32, f32),
}

impl ChatPanel {
    pub(crate) fn sync(&mut self, revision: u64, messages: impl FnOnce() -> Vec<ChatMessage>) {
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.messages = messages();
        self.heights.clear();
        self.scroll = 0.0;
    }

    pub(crate) fn is_empty_input(&self) -> bool {
        self.input.trim().is_empty() && self.preedit.is_empty()
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        self.input.insert_str(self.caret, &text);
        self.caret += text.len();
        self.preedit.clear();
    }

    pub(crate) fn set_preedit(&mut self, text: &str) {
        self.preedit = text.to_owned();
    }

    pub(crate) fn backspace(&mut self) {
        if let Some((index, _)) = self.input[..self.caret].char_indices().next_back() {
            self.input.replace_range(index..self.caret, "");
            self.caret = index;
        }
    }

    pub(crate) fn delete(&mut self) {
        if let Some(c) = self.input[self.caret..].chars().next() {
            self.input
                .replace_range(self.caret..self.caret + c.len_utf8(), "");
        }
    }

    pub(crate) fn move_caret(&mut self, forward: bool) {
        self.caret = if forward {
            self.input[self.caret..]
                .chars()
                .next()
                .map_or(self.caret, |c| self.caret + c.len_utf8())
        } else {
            self.input[..self.caret]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index)
        };
    }

    pub(crate) fn caret_to(&mut self, end: bool) {
        self.caret = if end { self.input.len() } else { 0 };
    }

    /// Takes the typed message for sending.
    pub(crate) fn take_input(&mut self) -> String {
        self.caret = 0;
        self.preedit.clear();
        std::mem::take(&mut self.input).trim().to_owned()
    }

    pub(crate) fn scroll_by(&mut self, lines: f32) {
        self.scroll += lines * WHEEL_STEP;
        self.clamp_scroll(self.heights_width);
    }

    pub(crate) fn scroll_to_end(&mut self) {
        self.scroll = 0.0;
    }

    /// Hit-tests a point in physical pixels relative to the panel's top-left corner.
    pub(crate) fn hit(&self, x: f32, y: f32, width: f32, height: f32, scale: f32) -> ChatHit {
        let layout = layout(width / scale, height / scale, self.voice);
        let point = Point::new(x / scale, y / scale);
        if layout.close.contains(point) {
            ChatHit::Close
        } else if layout.clear.contains(point) {
            ChatHit::Clear
        } else if layout.mic.is_some_and(|mic| mic.contains(point)) {
            ChatHit::Mic
        } else if layout.input.contains(point) {
            ChatHit::Input
        } else {
            ChatHit::Panel
        }
    }

    /// The text field in physical pixels relative to the panel, for placing the IME window.
    pub(crate) fn input_rect(&self, width: f32, height: f32, scale: f32) -> Rect {
        let input = layout(width / scale, height / scale, self.voice).input;
        Rect::from_xywh(
            input.left * scale,
            input.top * scale,
            input.width() * scale,
            input.height() * scale,
        )
    }

    fn user_lines(text: &str, content_w: f32) -> Vec<String> {
        FontManager::global().wrap_text(
            text,
            TEXT_SIZE,
            FontWeight::REGULAR,
            (content_w * USER_MAX_WIDTH - BUBBLE_PAD_X * 2.0).max(TEXT_SIZE),
        )
    }

    fn measure(&mut self, content_w: f32) {
        if self.heights.len() == self.messages.len() && self.heights_width == content_w {
            return;
        }
        self.heights_width = content_w;
        self.heights = self
            .messages
            .iter()
            .map(|message| {
                if message.from_user {
                    Self::user_lines(&message.text, content_w).len().max(1) as f32 * LINE_HEIGHT
                        + BUBBLE_PAD_Y * 2.0
                } else {
                    markdown::markdown_height(&message.text, content_w)
                }
            })
            .collect();
    }

    fn content_height(&self) -> f32 {
        let gaps = self.heights.len().saturating_sub(1) as f32 * MESSAGE_GAP;
        let thinking = if self.thinking {
            MESSAGE_GAP + THINKING_H
        } else {
            0.0
        };
        self.heights.iter().sum::<f32>() + gaps + thinking
    }

    fn clamp_scroll(&mut self, content_w: f32) {
        if content_w <= 0.0 {
            return;
        }
        let area = self.area_height;
        self.scroll = self
            .scroll
            .clamp(0.0, (self.content_height() - area).max(0.0));
    }

    /// Lays the panel out for its full open size in physical pixels.
    pub(crate) fn prepare(&mut self, width: f32, height: f32, scale: f32) {
        self.size = (width / scale, height / scale);
        let layout = layout(self.size.0, self.size.1, self.voice);
        self.measure(layout.content_w);
        self.area_height = layout.area_bottom - layout.area_top;
        self.clamp_scroll(layout.content_w);
    }

    /// Draws into the island `rect`, which grows towards the prepared size while opening.
    pub(crate) fn draw(&self, painter: Painter<'_>, rect: Rect, scale: f32, alpha: f32, time: f32) {
        if alpha <= 0.001 || rect.width() <= 0.0 || self.size.0 <= 0.0 {
            return;
        }
        let layout = layout(self.size.0, self.size.1, self.voice);
        let fade = |color: Rgba| color.with_alpha_f(f32::from(color.a()) / 255.0 * alpha);
        let base = painter.save();
        painter.clip_rect(rect);
        painter.translate(Vec2::new(
            rect.left + (rect.width() - self.size.0 * scale) * 0.5,
            rect.top,
        ));
        painter.scale(Vec2::new(scale, scale));

        draw_text(
            painter,
            &tr("lingosoul_chat_title"),
            PAD + 4.0,
            TOP_PAD + (HEADER_H - 6.0) * 0.5 + TITLE_SIZE * 0.36,
            TITLE_SIZE,
            FontWeight::SEMIBOLD,
            fade(TEXT),
        );
        for (button, label) in [
            (layout.clear, "lingosoul_chat_clear"),
            (layout.close, "lingosoul_chat_close"),
        ] {
            painter.fill_round_rect(button, Radius::uniform(button.height() / 2.0), fade(BUTTON));
            draw_text(
                painter,
                &tr(label),
                button.left + 10.0,
                button.top + button.height() * 0.5 + BUTTON_SIZE * 0.36,
                BUTTON_SIZE,
                FontWeight::MEDIUM,
                fade(SECONDARY),
            );
        }

        let area = Rect::from_xywh(
            0.0,
            layout.area_top,
            layout.width,
            layout.area_bottom - layout.area_top,
        );
        let clip = painter.save();
        painter.clip_rect(area);
        let total = self.content_height();
        let mut y = if total <= area.height() {
            area.top
        } else {
            area.bottom - total + self.scroll
        };
        if self.messages.is_empty() && !self.thinking {
            let hint = tr(if self.connected {
                "lingosoul_chat_empty"
            } else {
                "lingosoul_chat_offline"
            });
            let width =
                FontManager::global().measure_text_cached(&hint, TEXT_SIZE, FontWeight::REGULAR);
            draw_text(
                painter,
                &hint,
                (layout.width - width).max(0.0) * 0.5,
                area.top + area.height() * 0.45,
                TEXT_SIZE,
                FontWeight::REGULAR,
                fade(SECONDARY),
            );
        }
        let colors = MarkdownColors {
            text: fade(TEXT),
            secondary: fade(SECONDARY),
            accent: fade(Rgba::from_rgb(130, 190, 255)),
            code_background: fade(Rgba::WHITE.with_alpha(26)),
            quote_background: fade(Rgba::WHITE.with_alpha(18)),
            separator: fade(Rgba::WHITE.with_alpha(46)),
        };
        for (message, height) in self.messages.iter().zip(&self.heights) {
            if y + height >= area.top && y <= area.bottom {
                if message.from_user {
                    let lines = Self::user_lines(&message.text, layout.content_w);
                    let widest = lines
                        .iter()
                        .map(|line| {
                            FontManager::global().measure_text_cached(
                                line,
                                TEXT_SIZE,
                                FontWeight::REGULAR,
                            )
                        })
                        .fold(0.0_f32, f32::max);
                    let bubble_w = widest + BUBBLE_PAD_X * 2.0;
                    let bubble =
                        Rect::from_xywh(PAD + layout.content_w - bubble_w, y, bubble_w, *height);
                    painter.fill_round_rect(
                        bubble,
                        Radius::uniform(14.0_f32.min(height / 2.0)),
                        fade(USER_BUBBLE),
                    );
                    for (i, line) in lines.iter().enumerate() {
                        draw_text(
                            painter,
                            line,
                            bubble.left + BUBBLE_PAD_X,
                            y + BUBBLE_PAD_Y + LINE_HEIGHT * i as f32 + TEXT_SIZE * 1.05,
                            TEXT_SIZE,
                            FontWeight::REGULAR,
                            fade(Rgba::WHITE),
                        );
                    }
                } else {
                    markdown::render(MarkdownRenderParams {
                        painter,
                        markdown: &message.text,
                        origin: (PAD, y),
                        width: layout.content_w,
                        visible_range: (area.top, area.bottom),
                        colors: MarkdownColors { ..colors },
                    });
                }
            }
            y += height + MESSAGE_GAP;
        }
        if self.thinking {
            for i in 0..3 {
                let pulse = 0.35 + 0.65 * (0.5 + 0.5 * (time * 5.0 - i as f32 * 0.8).sin());
                painter.fill_circle(
                    Point::new(PAD + 6.0 + i as f32 * 12.0, y + THINKING_H * 0.5),
                    3.5,
                    fade(Rgba::WHITE.with_alpha((255.0 * pulse) as u8)),
                );
            }
        }
        painter.restore_to(clip);

        let input = layout.input;
        painter.fill_round_rect(
            input,
            Radius::uniform(input.height() / 2.0),
            fade(if self.focused { FIELD_FOCUSED } else { FIELD }),
        );
        let text_x = input.left + 14.0;
        let field_w = layout.mic.map_or(input.right, |mic| mic.left - 4.0) - text_x - 10.0;
        if let Some(mic) = layout.mic {
            self.draw_mic(painter, mic, alpha, time);
        }
        let baseline = input.top + input.height() * 0.5 + TEXT_SIZE * 0.36;
        let field = painter.save();
        painter.clip_rect(Rect::from_xywh(
            text_x - 2.0,
            input.top,
            field_w + 4.0,
            input.height(),
        ));
        if self.input.is_empty() && self.preedit.is_empty() {
            draw_text(
                painter,
                &tr(if self.listening {
                    "lingosoul_chat_listening"
                } else {
                    "lingosoul_chat_placeholder"
                }),
                text_x,
                baseline,
                TEXT_SIZE,
                FontWeight::REGULAR,
                fade(SECONDARY),
            );
            if self.focused {
                draw_caret(painter, text_x, baseline, fade(TEXT));
            }
        } else {
            let before = &self.input[..self.caret];
            let after = &self.input[self.caret..];
            let measure = |text: &str| {
                FontManager::global().measure_text_cached(text, TEXT_SIZE, FontStyle::normal())
            };
            let caret_x = measure(before) + measure(&self.preedit);
            let shift = (caret_x - field_w + 4.0).max(0.0);
            let mut x = text_x - shift;
            draw_text(
                painter,
                before,
                x,
                baseline,
                TEXT_SIZE,
                FontWeight::REGULAR,
                fade(TEXT),
            );
            x += measure(before);
            if !self.preedit.is_empty() {
                draw_text(
                    painter,
                    &self.preedit,
                    x,
                    baseline,
                    TEXT_SIZE,
                    FontWeight::REGULAR,
                    fade(TEXT),
                );
                let width = measure(&self.preedit);
                painter.fill_rect(
                    Rect::from_xywh(x, baseline + 3.0, width, 1.0),
                    fade(SECONDARY),
                );
                x += width;
            }
            if self.focused {
                draw_caret(painter, x, baseline, fade(TEXT));
            }
            draw_text(
                painter,
                after,
                x,
                baseline,
                TEXT_SIZE,
                FontWeight::REGULAR,
                fade(TEXT),
            );
        }
        painter.restore_to(field);
        painter.restore_to(base);
    }
}

impl ChatPanel {
    /// The voice button: a microphone, or a stop button with a ring that follows the voice level.
    fn draw_mic(&self, painter: Painter<'_>, rect: Rect, alpha: f32, time: f32) {
        let center = Point::new(
            rect.left + rect.width() / 2.0,
            rect.top + rect.height() / 2.0,
        );
        let radius = rect.width() / 2.0;
        let fade = |color: Rgba, factor: f32| {
            color.with_alpha_f(f32::from(color.a()) / 255.0 * alpha * factor)
        };
        if self.listening {
            let breathe = 0.5 + 0.5 * (time * 3.0).sin();
            let ring = radius + 2.0 + 5.0 * self.level.max(0.15 * breathe);
            painter.fill_circle(center, ring, fade(LISTENING, 0.28));
            painter.fill_circle(center, radius, fade(LISTENING, 1.0));
        } else {
            painter.fill_circle(center, radius, fade(BUTTON, 1.0));
        }
        let icon = if self.listening {
            &STOP_ICON
        } else {
            &MIC_ICON
        };
        icon.with(|icon| {
            let Some((image, aspect)) = icon.as_ref() else {
                return;
            };
            let height = radius * if self.listening { 0.8 } else { 1.0 };
            let width = height * aspect;
            painter.draw_image(
                image,
                Rect::from_xywh(
                    center.x - width / 2.0,
                    center.y - height / 2.0,
                    width,
                    height,
                ),
                &ImageOptions::default()
                    .with_sampling(Sampling::LinearLinear)
                    .with_alpha((alpha * if self.listening { 255.0 } else { 220.0 }) as u8),
            );
        });
    }
}

fn draw_caret(painter: Painter<'_>, x: f32, baseline: f32, color: Rgba) {
    painter.fill_rect(
        Rect::from_xywh(x - 2.0, baseline - TEXT_SIZE, 1.5, TEXT_SIZE * 1.3),
        color,
    );
}

fn draw_text(
    painter: Painter<'_>,
    text: &str,
    x: f32,
    baseline: f32,
    size: f32,
    weight: FontWeight,
    color: Rgba,
) {
    FontManager::global().draw_text_cached(DrawTextCachedParams {
        painter,
        text,
        x,
        y: baseline,
        size,
        weight,
        color,
        blur: None,
    });
}
