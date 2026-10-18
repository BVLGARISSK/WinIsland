use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use winisland_core::i18n::tr;
use winisland_platform::{AudioOutputDevice, AudioOutputKind};
use winisland_render::text::{DrawTextCachedParams, FontManager};
use winisland_render::{
    FontWeight, Image, ImageOptions, Painter, Point, Radius, Rect, Rgba, Sampling, Vec2,
};

use super::{
    CONTENT_PADDING, COVER_SIZE, PLAYBACK_CONTROLS_TOP_GAP, PROGRESS_BAR_HEIGHT, PROGRESS_TIME_GAP,
    PROGRESS_TIME_WIDTH, PROGRESS_TOP_GAP, SKIP_BUTTON_GAP,
};

const SIDE_BUTTON_SLOTS: f32 = 2.0;
const SIDE_ICON_HEIGHT: f32 = 21.0;
const SIDE_BUTTON_HIT: f32 = 38.0;
const SIDE_ICON_ALPHA: f32 = 0.55;
const PANEL_RESPONSE: f32 = 0.28;
const VOLUME_IDLE_TIMEOUT: Duration = Duration::from_secs(3);
const VOLUME_SETTLE: Duration = Duration::from_millis(400);
const OUTPUT_CLOSE_DELAY: Duration = Duration::from_millis(380);
const ROW_HEIGHT: f32 = 32.0;
const ROW_ICON_HEIGHT: f32 = 15.0;
const ROW_TEXT_SIZE: f32 = 13.0;
const LIST_INSET: f32 = 8.0;
const SCROLL_GUTTER: f32 = 12.0;
const SCROLL_THUMB_WIDTH: f32 = 3.0;
const SCROLL_THUMB_MIN: f32 = 18.0;
const VOLUME_ICON_HEIGHT: f32 = 11.0;
const VOLUME_WHEEL_STEP: f32 = 0.04;
const VOLUME_GLYPH_FADE: Duration = Duration::from_millis(220);
const VOLUME_GLYPH_START_SCALE: f32 = 0.92;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AudioPanel {
    #[default]
    None,
    Outputs,
    Volume,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Glyph {
    AirPlay,
    Mute,
    VolumeOne,
    VolumeTwo,
    VolumeLow,
    VolumeHigh,
    Headphones,
    Speaker,
    Display,
    Check,
}

impl Glyph {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::AirPlay => include_bytes!("../../../../resources/in_app/icons/airplay.audio.png"),
            Self::Mute => {
                include_bytes!("../../../../resources/in_app/icons/speaker.slash.fill.png")
            }
            Self::VolumeOne => {
                include_bytes!("../../../../resources/in_app/icons/speaker.wave.1.fill.png")
            }
            Self::VolumeTwo => {
                include_bytes!("../../../../resources/in_app/icons/speaker.wave.2.fill.png")
            }
            Self::VolumeLow => {
                include_bytes!("../../../../resources/in_app/icons/speaker.fill.png")
            }
            Self::VolumeHigh => {
                include_bytes!("../../../../resources/in_app/icons/speaker.wave.3.fill.png")
            }
            Self::Headphones => include_bytes!("../../../../resources/in_app/icons/headphones.png"),
            Self::Speaker => {
                include_bytes!("../../../../resources/in_app/icons/hifispeaker.fill.png")
            }
            Self::Display => include_bytes!("../../../../resources/in_app/icons/display.png"),
            Self::Check => include_bytes!("../../../../resources/in_app/icons/checkmark.png"),
        }
    }

    fn for_output(kind: AudioOutputKind) -> Self {
        match kind {
            AudioOutputKind::Headphones => Self::Headphones,
            AudioOutputKind::Display => Self::Display,
            AudioOutputKind::Speakers => Self::Speaker,
        }
    }

    fn for_volume(volume: Option<(f32, bool)>) -> Self {
        match volume {
            None => Self::VolumeTwo,
            Some((level, muted)) if muted || level <= 0.005 => Self::Mute,
            Some((level, _)) if level < 0.34 => Self::VolumeOne,
            Some((level, _)) if level < 0.67 => Self::VolumeTwo,
            Some(_) => Self::VolumeHigh,
        }
    }
}

type GlyphCache = HashMap<(Glyph, u32), Option<(Image, f32)>>;

struct State {
    panel: AudioPanel,
    outputs_t: f32,
    volume_t: f32,
    devices: Vec<AudioOutputDevice>,
    volume: Option<(f32, bool)>,
    volume_hold_until: Option<Instant>,
    pointer: Option<Point>,
    list_scroll: f32,
    list_drag: Option<f32>,
    reveal_default: bool,
    volume_glyph: Option<(Glyph, Option<Glyph>, Instant)>,
    volume_touched: Instant,
    close_at: Option<Instant>,
    volume_dragging: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        panel: AudioPanel::None,
        outputs_t: 0.0,
        volume_t: 0.0,
        devices: Vec::new(),
        volume: None,
        volume_hold_until: None,
        pointer: None,
        list_scroll: 0.0,
        list_drag: None,
        reveal_default: false,
        volume_glyph: None,
        volume_touched: Instant::now(),
        close_at: None,
        volume_dragging: false,
    });
    static GLYPHS: RefCell<GlyphCache> = RefCell::new(HashMap::new());
}

fn glyph(kind: Glyph, color: Rgba) -> Option<(Image, f32)> {
    let color = Rgba::from_rgb(color.r(), color.g(), color.b());
    GLYPHS.with(|cache| {
        cache
            .borrow_mut()
            .entry((
                kind,
                u32::from_be_bytes([0, color.r(), color.g(), color.b()]),
            ))
            .or_insert_with(|| crate::icons::load_tinted_glyph(kind.bytes(), color))
            .clone()
    })
}

fn draw_glyph_at(painter: Painter<'_>, kind: Glyph, rect: Rect, color: Rgba) {
    let Some((image, _)) = glyph(kind, color).filter(|_| color.a() > 0) else {
        return;
    };
    painter.draw_image(
        &image,
        rect,
        &ImageOptions::default()
            .with_sampling(Sampling::LinearLinear)
            .with_alpha(color.a()),
    );
}

fn glyph_width(kind: Glyph, height: f32) -> f32 {
    glyph(kind, Rgba::WHITE).map_or(height, |(_, aspect)| height * aspect)
}

fn draw_glyph(painter: Painter<'_>, kind: Glyph, center: Point, height: f32, color: Rgba) {
    let width = glyph_width(kind, height);
    draw_glyph_at(
        painter,
        kind,
        Rect::from_xywh(
            center.x - width / 2.0,
            center.y - height / 2.0,
            width,
            height,
        ),
        color,
    );
}

/// Draws a speaker glyph anchored at the speaker body so the waves grow to the right.
fn draw_volume_glyph(
    painter: Painter<'_>,
    kind: Glyph,
    center: Point,
    height: f32,
    zoom: f32,
    color: Rgba,
) {
    let left = center.x - glyph_width(Glyph::VolumeHigh, height) / 2.0;
    let height = height * zoom;
    let width = glyph_width(kind, height);
    draw_glyph_at(
        painter,
        kind,
        Rect::from_xywh(left, center.y - height / 2.0, width, height),
        color,
    );
}

/// Tracks the speaker glyph for `volume`, returning the current glyph, the one it replaces,
/// and how far the crossfade between them has progressed.
fn volume_glyph_transition(state: &mut State) -> (Glyph, Option<Glyph>, f32) {
    let target = Glyph::for_volume(state.volume);
    let now = Instant::now();
    let (current, previous, started) = match state.volume_glyph {
        Some((current, _, _)) if current != target => (target, Some(current), now),
        Some(transition) => transition,
        None => (
            target,
            None,
            now.checked_sub(VOLUME_GLYPH_FADE).unwrap_or(now),
        ),
    };
    state.volume_glyph = Some((current, previous, started));
    let progress =
        (now.duration_since(started).as_secs_f32() / VOLUME_GLYPH_FADE.as_secs_f32()).min(1.0);
    let eased = 1.0 - (1.0 - progress).powi(3);
    (current, previous.filter(|_| progress < 1.0), eased)
}

fn scaled(alpha: u8, factor: f32) -> u8 {
    (alpha as f32 * factor.clamp(0.0, 1.0)).round() as u8
}

/// Shared vertical anchors of the playback area.
pub(super) struct Lanes {
    pub(super) bar_y: f32,
    pub(super) controls_y: f32,
    pub(super) bar_full_left: f32,
    pub(super) bar_full_right: f32,
    pub(super) bar_left: f32,
    pub(super) bar_right: f32,
}

pub(super) fn lanes(ox: f32, oy: f32, w: f32, scale: f32) -> Lanes {
    let bar_y = oy + (CONTENT_PADDING + COVER_SIZE + PROGRESS_TOP_GAP) * scale;
    let bar_full_left = ox + CONTENT_PADDING * scale;
    let bar_full_right = ox + w - CONTENT_PADDING * scale;
    let time_w = (PROGRESS_TIME_WIDTH + PROGRESS_TIME_GAP) * scale;
    Lanes {
        bar_y,
        controls_y: bar_y + (PROGRESS_BAR_HEIGHT / 2.0 + PLAYBACK_CONTROLS_TOP_GAP) * scale,
        bar_full_left,
        bar_full_right,
        bar_left: bar_full_left + time_w,
        bar_right: bar_full_right - time_w,
    }
}

fn side_center(ox: f32, oy: f32, w: f32, scale: f32, direction: f32) -> Point {
    Point::new(
        ox + w / 2.0 + direction * SKIP_BUTTON_GAP * SIDE_BUTTON_SLOTS * scale,
        lanes(ox, oy, w, scale).controls_y,
    )
}

fn hit_rect(center: Point, scale: f32) -> Rect {
    let size = SIDE_BUTTON_HIT * scale;
    Rect::from_xywh(center.x - size / 2.0, center.y - size / 2.0, size, size)
}

pub fn output_button_rect(ox: f32, oy: f32, w: f32, scale: f32) -> Rect {
    hit_rect(side_center(ox, oy, w, scale, -1.0), scale)
}

pub fn volume_button_rect(ox: f32, oy: f32, w: f32, scale: f32) -> Rect {
    hit_rect(side_center(ox, oy, w, scale, 1.0), scale)
}

/// The whole volume row, including the speaker glyphs at both ends.
pub fn volume_bar_rect(ox: f32, oy: f32, w: f32, scale: f32) -> Rect {
    let lanes = lanes(ox, oy, w, scale);
    let hit = 20.0 * scale;
    Rect::from_ltrb(
        lanes.bar_full_left,
        lanes.bar_y - hit / 2.0,
        lanes.bar_full_right,
        lanes.bar_y + hit / 2.0,
    )
}

pub fn output_list_rect(ox: f32, oy: f32, w: f32, scale: f32) -> Rect {
    let lanes = lanes(ox, oy, w, scale);
    let top = oy + (CONTENT_PADDING - LIST_INSET) * scale;
    let row_h = ROW_HEIGHT * scale;
    let rows = ((lanes.bar_y + 14.0 * scale - top) / row_h)
        .floor()
        .max(1.0);
    let height = rows * row_h;
    let top = top + (lanes.bar_y + 14.0 * scale - top - height) / 2.0;
    Rect::from_ltrb(
        ox + (CONTENT_PADDING - LIST_INSET) * scale,
        top,
        ox + w - (CONTENT_PADDING - LIST_INSET) * scale,
        top + height,
    )
}

struct ListLayout {
    rows: Rect,
    max_scroll: f32,
    track: Option<Rect>,
}

fn list_layout(state: &State, list: Rect, scale: f32) -> ListLayout {
    let max_scroll = (state.devices.len() as f32 * ROW_HEIGHT * scale - list.height()).max(0.0);
    if max_scroll <= 0.0 {
        return ListLayout {
            rows: list,
            max_scroll,
            track: None,
        };
    }
    let gutter = SCROLL_GUTTER * scale;
    ListLayout {
        rows: Rect::from_ltrb(list.left, list.top, list.right - gutter, list.bottom),
        max_scroll,
        track: Some(Rect::from_ltrb(
            list.right - gutter,
            list.top,
            list.right,
            list.bottom,
        )),
    }
}

fn thumb_rect(track: Rect, list: Rect, scroll: f32, max_scroll: f32, scale: f32) -> Rect {
    let inset = 3.0 * scale;
    let travel_h = track.height() - inset * 2.0;
    let height = (travel_h * list.height() / (list.height() + max_scroll))
        .max(SCROLL_THUMB_MIN * scale)
        .min(travel_h);
    let y = track.top + inset + (travel_h - height) * (scroll / max_scroll).clamp(0.0, 1.0);
    let width = SCROLL_THUMB_WIDTH * scale;
    Rect::from_xywh(track.center_x() - width / 2.0, y, width, height)
}

pub fn output_row_at(ox: f32, oy: f32, w: f32, scale: f32, point: Point) -> Option<String> {
    let list = output_list_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let state = state.borrow();
        let layout = list_layout(&state, list, scale);
        if !layout.rows.contains(point) {
            return None;
        }
        let index = ((point.y - list.top + state.list_scroll) / (ROW_HEIGHT * scale)) as usize;
        state.devices.get(index).map(|device| device.id.clone())
    })
}

/// Starts dragging the list's scroll thumb; pressing the track jumps the thumb there first.
pub fn begin_output_scroll_drag(ox: f32, oy: f32, w: f32, scale: f32, point: Point) -> bool {
    let list = output_list_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let layout = list_layout(&state, list, scale);
        let Some(track) = layout.track.filter(|track| track.contains(point)) else {
            return false;
        };
        let thumb = thumb_rect(track, list, state.list_scroll, layout.max_scroll, scale);
        let grab = if point.y >= thumb.top && point.y <= thumb.bottom {
            point.y - thumb.top
        } else {
            thumb.height() / 2.0
        };
        state.list_drag = Some(grab);
        drop(state);
        drag_output_scroll(ox, oy, w, scale, point.y);
        true
    })
}

pub fn drag_output_scroll(ox: f32, oy: f32, w: f32, scale: f32, y: f32) {
    let list = output_list_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(grab) = state.list_drag else {
            return;
        };
        let layout = list_layout(&state, list, scale);
        let Some(track) = layout.track else {
            return;
        };
        let thumb = thumb_rect(track, list, state.list_scroll, layout.max_scroll, scale);
        let inset = 3.0 * scale;
        let travel = track.height() - inset * 2.0 - thumb.height();
        if travel > 0.0 {
            let ratio = ((y - grab - track.top - inset) / travel).clamp(0.0, 1.0);
            state.list_scroll = ratio * layout.max_scroll;
        }
    });
}

pub fn end_output_scroll_drag() {
    STATE.with(|state| state.borrow_mut().list_drag = None);
}

pub fn output_scroll_dragging() -> bool {
    STATE.with(|state| state.borrow().list_drag.is_some())
}

pub fn set_audio_state(devices: Option<Vec<AudioOutputDevice>>, volume: Option<(f32, bool)>) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(devices) = devices {
            state.devices = devices;
        }
        let holding = state
            .volume_hold_until
            .is_some_and(|until| Instant::now() < until);
        if !state.volume_dragging && !holding {
            state.volume = volume;
            state.volume_hold_until = None;
        }
    });
}

pub fn set_audio_pointer(pointer: Option<Point>) {
    STATE.with(|state| state.borrow_mut().pointer = pointer);
}

pub fn audio_panel() -> AudioPanel {
    STATE.with(|state| state.borrow().panel)
}

pub fn toggle_audio_panel(panel: AudioPanel) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.close_at = None;
        state.list_scroll = 0.0;
        state.list_drag = None;
        state.reveal_default = true;
        state.volume_touched = Instant::now();
        state.panel = if state.panel == panel {
            AudioPanel::None
        } else {
            panel
        };
    });
}

pub fn close_audio_panel_soon() {
    STATE.with(|state| state.borrow_mut().close_at = Some(Instant::now() + OUTPUT_CLOSE_DELAY));
}

pub fn audio_controls_animating() -> bool {
    STATE.with(|state| {
        let state = state.borrow();
        let outputs = if state.panel == AudioPanel::Outputs {
            1.0
        } else {
            0.0
        };
        let volume = if state.panel == AudioPanel::Volume {
            1.0
        } else {
            0.0
        };
        state.panel != AudioPanel::None
            || (state.outputs_t - outputs).abs() > 0.001
            || (state.volume_t - volume).abs() > 0.001
            || state
                .volume_glyph
                .is_some_and(|(_, _, started)| started.elapsed() < VOLUME_GLYPH_FADE)
    })
}

/// Sets the volume from a pointer position on the volume bar and returns the new level.
pub fn volume_level_at(ox: f32, oy: f32, w: f32, scale: f32, x: f32) -> f32 {
    let lanes = lanes(ox, oy, w, scale);
    let level =
        ((x - lanes.bar_left) / (lanes.bar_right - lanes.bar_left).max(1.0)).clamp(0.0, 1.0);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.volume = Some((level, level <= 0.0));
        state.volume_touched = Instant::now();
        state.volume_hold_until = Some(Instant::now() + VOLUME_SETTLE);
    });
    level
}

pub fn set_volume_dragging(dragging: bool) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.volume_dragging = dragging;
        state.volume_touched = Instant::now();
        state.volume_hold_until = Some(Instant::now() + VOLUME_SETTLE);
    });
}

/// Steps the volume for a wheel notch and returns the new level, opening the slider.
pub fn step_volume(notches: f32) -> Option<f32> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let (level, _) = state.volume?;
        let level = (level + notches * VOLUME_WHEEL_STEP).clamp(0.0, 1.0);
        state.volume = Some((level, level <= 0.0));
        state.volume_touched = Instant::now();
        state.volume_hold_until = Some(Instant::now() + VOLUME_SETTLE);
        state.panel = AudioPanel::Volume;
        state.close_at = None;
        Some(level)
    })
}

pub fn scroll_output_list(ox: f32, oy: f32, w: f32, scale: f32, delta: f32) {
    let list = output_list_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let max = list_layout(&state, list, scale).max_scroll;
        state.list_scroll = (state.list_scroll - delta).clamp(0.0, max);
    });
}

/// Advances panel animations; returns how far the output list and the volume slider are shown.
pub(super) fn tick(
    dt: f32,
    expansion_progress: f32,
    ox: f32,
    oy: f32,
    w: f32,
    scale: f32,
) -> (f32, f32) {
    let volume_row = volume_bar_rect(ox, oy, w, scale);
    let volume_button = volume_button_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let now = Instant::now();
        if state.pointer.is_some_and(|point| {
            state.panel == AudioPanel::Volume
                && (volume_row.contains(point) || volume_button.contains(point))
        }) {
            state.volume_touched = now;
        }
        if expansion_progress < 0.95
            || state.close_at.is_some_and(|close_at| now >= close_at)
            || (state.panel == AudioPanel::Volume
                && !state.volume_dragging
                && now.duration_since(state.volume_touched) > VOLUME_IDLE_TIMEOUT)
        {
            state.panel = AudioPanel::None;
            state.close_at = None;
            state.volume_dragging = false;
            state.list_drag = None;
        }
        let factor = 1.0 - (1.0 - PANEL_RESPONSE).powf(dt.max(0.0));
        let ease = |value: &mut f32, target: f32| {
            *value += (target - *value) * factor;
            if (*value - target).abs() < 0.002 {
                *value = target;
            }
        };
        let outputs = if state.panel == AudioPanel::Outputs {
            1.0
        } else {
            0.0
        };
        let volume = if state.panel == AudioPanel::Volume {
            1.0
        } else {
            0.0
        };
        ease(&mut state.outputs_t, outputs);
        ease(&mut state.volume_t, volume);
        (state.outputs_t, state.volume_t)
    })
}

pub(super) struct SideButtonsParams<'a> {
    pub(super) painter: Painter<'a>,
    pub(super) ox: f32,
    pub(super) oy: f32,
    pub(super) w: f32,
    pub(super) scale: f32,
    pub(super) alpha: u8,
    pub(super) text_color: Rgba,
}

pub(super) fn draw_side_buttons(params: SideButtonsParams<'_>) {
    let SideButtonsParams {
        painter,
        ox,
        oy,
        w,
        scale,
        alpha,
        text_color,
    } = params;
    let (panel, pointer, output, volume) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let volume = volume_glyph_transition(&mut state);
        let output = state
            .devices
            .iter()
            .find(|device| device.is_default)
            .map_or(Glyph::AirPlay, |device| Glyph::for_output(device.kind));
        (state.panel, state.pointer, output, volume)
    });
    let emphasis = |center: Point, active: bool| {
        if active {
            1.0
        } else if pointer.is_some_and(|point| hit_rect(center, scale).contains(point)) {
            0.8
        } else {
            SIDE_ICON_ALPHA
        }
    };
    let height = SIDE_ICON_HEIGHT * scale;
    let output_center = side_center(ox, oy, w, scale, -1.0);
    draw_glyph(
        painter,
        output,
        output_center,
        height,
        text_color.with_alpha(scaled(
            alpha,
            emphasis(output_center, panel == AudioPanel::Outputs),
        )),
    );
    let volume_center = side_center(ox, oy, w, scale, 1.0);
    let volume_alpha = scaled(alpha, emphasis(volume_center, panel == AudioPanel::Volume));
    let (current, previous, progress) = volume;
    if let Some(previous) = previous {
        draw_volume_glyph(
            painter,
            previous,
            volume_center,
            height,
            1.0,
            text_color.with_alpha(scaled(volume_alpha, 1.0 - progress)),
        );
    }
    let (zoom, fade) = if previous.is_some() {
        (
            VOLUME_GLYPH_START_SCALE + (1.0 - VOLUME_GLYPH_START_SCALE) * progress,
            progress,
        )
    } else {
        (1.0, 1.0)
    };
    draw_volume_glyph(
        painter,
        current,
        volume_center,
        height,
        zoom,
        text_color.with_alpha(scaled(volume_alpha, fade)),
    );
}

pub(super) struct VolumeRowParams<'a> {
    pub(super) painter: Painter<'a>,
    pub(super) lanes: &'a Lanes,
    pub(super) bar_h: f32,
    pub(super) scale: f32,
    pub(super) alpha: u8,
    pub(super) text_color: Rgba,
    pub(super) track_color: Rgba,
}

pub(super) fn draw_volume_row(params: VolumeRowParams<'_>) {
    let VolumeRowParams {
        painter,
        lanes,
        bar_h,
        scale,
        alpha,
        text_color,
        track_color,
    } = params;
    if alpha == 0 {
        return;
    }
    let level = STATE.with(|state| {
        state
            .borrow()
            .volume
            .map_or(0.0, |(level, muted)| if muted { 0.0 } else { level })
    });
    let icon_color = text_color.with_alpha(scaled(alpha, SIDE_ICON_ALPHA));
    let icon_h = VOLUME_ICON_HEIGHT * scale;
    let time_w = PROGRESS_TIME_WIDTH * scale;
    draw_glyph(
        painter,
        Glyph::VolumeLow,
        Point::new(lanes.bar_full_left + time_w / 2.0, lanes.bar_y),
        icon_h,
        icon_color,
    );
    draw_glyph(
        painter,
        Glyph::VolumeHigh,
        Point::new(lanes.bar_full_right - time_w / 2.0, lanes.bar_y),
        icon_h,
        icon_color,
    );
    let track = Rect::from_ltrb(
        lanes.bar_left,
        lanes.bar_y - bar_h / 2.0,
        lanes.bar_right,
        lanes.bar_y + bar_h / 2.0,
    );
    let radius = Radius::uniform(bar_h / 2.0);
    painter.fill_round_rect(track, radius, track_color);
    let filled = track.width() * level;
    if filled > 0.0 {
        painter.save();
        painter.clip_round_rect(track, radius);
        painter.fill_round_rect(
            Rect::from_xywh(track.left, track.top, filled, bar_h),
            Radius::uniform(0.0),
            text_color.with_alpha(alpha),
        );
        painter.restore();
    }
}

pub(super) struct OutputListParams<'a> {
    pub(super) painter: Painter<'a>,
    pub(super) ox: f32,
    pub(super) oy: f32,
    pub(super) w: f32,
    pub(super) scale: f32,
    pub(super) alpha: u8,
    pub(super) text_color: Rgba,
    pub(super) accent: Rgba,
}

pub(super) fn draw_output_list(params: OutputListParams<'_>) {
    let OutputListParams {
        painter,
        ox,
        oy,
        w,
        scale,
        alpha,
        text_color,
        accent,
    } = params;
    if alpha == 0 {
        return;
    }
    let list = output_list_rect(ox, oy, w, scale);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let layout = list_layout(&state, list, scale);
        if state.reveal_default
            && let Some(index) = state.devices.iter().position(|device| device.is_default)
        {
            let row_h = ROW_HEIGHT * scale;
            state.list_scroll = index as f32 * row_h - (list.height() - row_h) / 2.0;
            state.reveal_default = false;
        }
        state.list_scroll = state.list_scroll.clamp(0.0, layout.max_scroll);
        let fonts = FontManager::global();
        if state.devices.is_empty() {
            let text = tr("audio_outputs_empty");
            let size = ROW_TEXT_SIZE * scale;
            let width = fonts.measure_text_cached(&text, size, FontWeight::REGULAR);
            fonts.draw_text_cached(DrawTextCachedParams {
                painter,
                text: &text,
                x: list.center_x() - width / 2.0,
                y: list.center_y() + size * 0.35,
                size,
                weight: FontWeight::REGULAR,
                color: text_color.with_alpha(scaled(alpha, SIDE_ICON_ALPHA)),
                blur: None,
            });
            return;
        }
        let row_h = ROW_HEIGHT * scale;
        let rows = layout.rows;
        painter.save();
        painter.clip_rect(rows);
        painter.translate(Vec2::new(0.0, -state.list_scroll));
        for (index, device) in state.devices.iter().enumerate() {
            let row = Rect::from_xywh(
                rows.left,
                rows.top + index as f32 * row_h,
                rows.width(),
                row_h,
            );
            if row.bottom < rows.top + state.list_scroll
                || row.top > rows.bottom + state.list_scroll
            {
                continue;
            }
            let hovered = state.list_drag.is_none()
                && state.pointer.is_some_and(|point| {
                    rows.contains(point)
                        && row.contains(Point::new(point.x, point.y + state.list_scroll))
                });
            if hovered || device.is_default {
                painter.fill_round_rect(
                    row.inset(2.0 * scale),
                    Radius::uniform(9.0 * scale),
                    text_color.with_alpha(scaled(alpha, if hovered { 0.12 } else { 0.07 })),
                );
            }
            let content_left = row.left + (LIST_INSET + 4.0) * scale;
            draw_glyph(
                painter,
                Glyph::for_output(device.kind),
                Point::new(content_left + 9.0 * scale, row.center_y()),
                ROW_ICON_HEIGHT * scale,
                text_color.with_alpha(scaled(alpha, if device.is_default { 1.0 } else { 0.7 })),
            );
            let check_x = row.right - (LIST_INSET + 12.0) * scale;
            let text_x = content_left + 28.0 * scale;
            let size = ROW_TEXT_SIZE * scale;
            let weight = if device.is_default {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::REGULAR
            };
            let name = fit_text(
                fonts,
                &device.name,
                size,
                weight,
                check_x - 14.0 * scale - text_x,
            );
            fonts.draw_text_cached(DrawTextCachedParams {
                painter,
                text: &name,
                x: text_x,
                y: row.center_y() + size * 0.35,
                size,
                weight,
                color: text_color
                    .with_alpha(scaled(alpha, if device.is_default { 1.0 } else { 0.85 })),
                blur: None,
            });
            if device.is_default {
                draw_glyph(
                    painter,
                    Glyph::Check,
                    Point::new(check_x, row.center_y()),
                    11.0 * scale,
                    accent.with_alpha(alpha),
                );
            }
        }
        painter.restore();
        if let Some(track) = layout.track {
            let active = state.list_drag.is_some()
                || state.pointer.is_some_and(|point| track.contains(point));
            let thumb = thumb_rect(track, list, state.list_scroll, layout.max_scroll, scale);
            let thumb = if active {
                Rect::from_xywh(
                    thumb.center_x() - thumb.width(),
                    thumb.top,
                    thumb.width() * 2.0,
                    thumb.height(),
                )
            } else {
                thumb
            };
            painter.fill_round_rect(
                thumb,
                Radius::uniform(thumb.width() / 2.0),
                text_color.with_alpha(scaled(alpha, if active { 0.55 } else { 0.3 })),
            );
        }
    });
}

fn fit_text(
    fonts: &FontManager,
    text: &str,
    size: f32,
    weight: FontWeight,
    max_width: f32,
) -> String {
    if fonts.measure_text_cached(text, size, weight) <= max_width {
        return text.to_string();
    }
    let mut fitted = String::new();
    for character in text.chars() {
        fitted.push(character);
        if fonts.measure_text_cached(&format!("{fitted}…"), size, weight) > max_width {
            fitted.pop();
            break;
        }
    }
    format!("{fitted}…")
}
