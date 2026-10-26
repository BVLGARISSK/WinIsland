mod geometry;
mod locale;

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use winisland_plugin_api::sdk::{
    CallbackResource, DrawListBuilder, Error, Event, Host, Rgba, Size, Surface, SystemApi,
    TextStyle,
};
use winisland_plugin_api::*;

use self::geometry::{Point, Rect, Vec2};
use self::locale::{Holiday, Region, days_in_month, weekday};

const ACCENT: Rgba = Rgba::from_argb(0xffff_453a);
const PANEL_RATIO: f32 = 0.36;
const DIVIDER_GAP: f32 = 16.0;
const HEADER_GAP: f32 = 4.0;
const BUTTON_SLOP: f32 = 3.0;
const TRANSITION_SECS: f32 = 0.32;
const PRESS_SECS: f32 = 0.26;
const PULSE_SECS: f32 = 0.45;
const HOVER_RATE: f32 = 18.0;
const HOLIDAY_CACHE_LIMIT: usize = 4;
const HOLIDAYS_TOOL: &str = "month_holidays";
const HOLIDAYS_DESCRIPTION: &str = "Lists the public holidays and festivals of one month for the user's region, using the calendar shown on the island. Omit year and month for the current month.";
const HOLIDAYS_SCHEMA: &str = r#"{"type":"object","properties":{"year":{"type":"integer","minimum":1601,"maximum":9999},"month":{"type":"integer","minimum":1,"maximum":12}}}"#;
const DAY_LABELS: [&str; 31] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17",
    "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
];

pub const ID: &str = "winisland-calendar";
pub const PAGE_KEY: &str = "calendar";

pub fn descriptor() -> PluginDescriptorV2 {
    PluginDescriptorV2 {
        struct_size: std::mem::size_of::<PluginDescriptorV2>() as u32,
        abi_version: ABI_VERSION_2,
        capabilities: CAP_SURFACE | CAP_INPUT | CAP_EVENTS | CAP_TEXT | CAP_SYSTEM | CAP_ASSISTANT,
        metadata: PluginMetadataC::new(
            ID,
            "Calendar",
            env!("CARGO_PKG_VERSION"),
            "WinIsland",
            "Calendar with month navigation and regional holidays.",
        ),
        create: Some(create),
        shutdown: Some(shutdown),
        destroy: Some(destroy),
        on_tick: Some(tick),
    }
}

struct Calendar {
    target: Option<Arc<CalendarTarget>>,
    callbacks: Vec<CallbackResource>,
}

struct CalendarTarget {
    host: Host,
    page: Surface,
}

impl Calendar {
    fn new(host: Host) -> Result<Self, Error> {
        let page = host.surfaces()?.create(SurfaceSpecV2 {
            kind: SURFACE_PAGE,
            flags: SURFACE_ENABLED,
            order: i32::MIN,
            width: 360.0,
            height: 200.0,
            key: str_to_fixed(PAGE_KEY),
            title: str_to_fixed("Calendar"),
            ..Default::default()
        })?;
        let target = Arc::new(CalendarTarget { host, page });
        reset_to_today();
        target.refresh()?;
        let events = target.host.events()?;
        let input_target = target.clone();
        let input = events.subscribe(
            EVENT_INPUT | EVENT_RESIZE | EVENT_VISIBILITY,
            target.page.id(),
            move |event| {
                if event.kind == EVENT_VISIBILITY && event.code == 0 {
                    reset_to_today();
                } else if event.kind == EVENT_INPUT {
                    handle_input(&event);
                }
                if let Err(error) = input_target.refresh() {
                    input_target
                        .host
                        .log()
                        .write(3, &format!("Calendar update failed: {error}"));
                }
            },
        )?;
        let timer_target = target.clone();
        let timer = events.timer(
            TimerSpecV2 {
                target: target.page.id(),
                flags: TIMER_VISIBLE_ONLY,
                delay_ms: 1000,
                interval_ms: 1000,
                ..Default::default()
            },
            move |_| {
                if let Err(error) = timer_target.refresh() {
                    timer_target
                        .host
                        .log()
                        .write(3, &format!("Calendar refresh failed: {error}"));
                }
            },
        )?;
        let mut callbacks = vec![input, timer];
        if let Ok(assistant) = target.host.assistant() {
            let host = target.host.clone();
            match assistant.register(
                HOLIDAYS_TOOL,
                HOLIDAYS_DESCRIPTION,
                HOLIDAYS_SCHEMA,
                move |args| month_holidays(&host, args).map_err(|error| error.to_string()),
            ) {
                Ok(tool) => callbacks.push(tool),
                Err(error) => target
                    .host
                    .log()
                    .write(2, &format!("Calendar assistant tool unavailable: {error}")),
            }
        }
        Ok(Self {
            target: Some(target),
            callbacks,
        })
    }
}

impl CalendarTarget {
    fn refresh(&self) -> Result<(), Error> {
        let (width, height) = self.page.logical_size();
        if width <= 0.0 || height <= 0.0 {
            return Ok(());
        }
        let layout = layout(0.0, 0.0, width, height, 1.0);
        let regions: Vec<_> = std::iter::once((CalendarAction::Today, layout.title))
            .chain(
                layout
                    .buttons
                    .into_iter()
                    .rev()
                    .map(|(action, rect)| (action, rect.inset(-BUTTON_SLOP))),
            )
            .filter(|(_, rect)| rect.width() > 0.0 && rect.height() > 0.0)
            .map(|(action, rect)| InputRegionV2 {
                id: action.index() as u64 + 1,
                x: rect.left,
                y: rect.top,
                width: rect.width(),
                height: rect.height(),
                flags: INPUT_POINTER | INPUT_KEYBOARD | INPUT_CAPTURE_ON_PRESS,
                ..Default::default()
            })
            .collect();
        self.host.input()?.set_regions(self.page.id(), &regions)?;
        let mut painter = CalendarPainter {
            host: &self.host,
            list: DrawListBuilder::new(Size::new(width, height)),
            clips: 0,
            error: None,
        };
        draw_calendar_page(&mut painter, 0.0, 0.0, width, height, 255, 1.0, Rgba::WHITE)?;
        if let Some(error) = painter.error {
            return Err(error);
        }
        self.page.submit(painter.list.finish())?;
        self.host
            .events()?
            .set_animation(self.page.id(), is_animating())
    }
}

fn handle_input(event: &Event) {
    let action = [
        CalendarAction::PreviousYear,
        CalendarAction::PreviousMonth,
        CalendarAction::NextMonth,
        CalendarAction::NextYear,
        CalendarAction::Today,
    ]
    .get(event.sequence.saturating_sub(1) as usize)
    .copied();
    match event.detail {
        INPUT_MOVE | INPUT_ENTER => {
            set_hover(action);
        }
        INPUT_LEAVE | INPUT_CANCEL | INPUT_BLUR => {
            set_hover(None);
        }
        INPUT_DOWN if event.code == 1 => {
            if let Some(action) = action {
                apply_action(action);
            }
        }
        INPUT_KEY_DOWN => {
            let action = match event.code {
                KEY_LEFT => Some(CalendarAction::PreviousMonth),
                KEY_RIGHT => Some(CalendarAction::NextMonth),
                KEY_UP => Some(CalendarAction::PreviousYear),
                KEY_DOWN => Some(CalendarAction::NextYear),
                KEY_ENTER => action,
                _ => None,
            };
            if let Some(action) = action {
                apply_action(action);
            }
        }
        _ => {}
    }
}

unsafe extern "C" fn create(
    info: *const PluginCreateInfoV2,
    out: *mut PluginHandleV2,
) -> PluginStatus {
    if info.is_null() || out.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host supplies a live create descriptor and writable handle output.
    let info = unsafe { &*info };
    // SAFETY: The host keeps its instance table allocated through shutdown and destruction.
    let host = match unsafe { Host::from_raw(info.host_api, info.plugin_token) } {
        Ok(host) => host,
        Err(_) => return PluginStatus::Internal,
    };
    match Calendar::new(host.clone()) {
        Ok(calendar) => {
            // SAFETY: The checked output receives an owned instance until destroy is called.
            unsafe {
                *out = Box::into_raw(Box::new(calendar)).cast();
            }
            PluginStatus::Ok
        }
        Err(error) => {
            host.log()
                .write(3, &format!("Cannot create built-in calendar: {error}"));
            PluginStatus::Internal
        }
    }
}

unsafe extern "C" fn shutdown(handle: *mut c_void) -> PluginStatus {
    if handle.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host has joined its worker and owns this live calendar instance.
    let calendar = unsafe { &mut *handle.cast::<Calendar>() };
    calendar.callbacks.clear();
    calendar.target.take();
    PluginStatus::Ok
}

unsafe extern "C" fn destroy(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: Successful shutdown stopped callbacks; the host destroys the instance once.
        drop(unsafe { Box::from_raw(handle.cast::<Calendar>()) });
    }
}

unsafe extern "C" fn tick(handle: *mut c_void, _: WidgetId, _: f64) -> PluginStatus {
    if handle.is_null() {
        return PluginStatus::InvalidArgument;
    }
    // SAFETY: The host worker holds the live instance until it is joined before shutdown.
    let calendar = unsafe { &*handle.cast::<Calendar>() };
    match calendar
        .target
        .as_ref()
        .ok_or(Error::StaleHandle)
        .and_then(|target| target.refresh())
    {
        Ok(()) => PluginStatus::Ok,
        Err(error) => {
            if let Some(target) = &calendar.target {
                target
                    .host
                    .log()
                    .write(3, &format!("Built-in calendar tick failed: {error}"));
            }
            PluginStatus::Internal
        }
    }
}

struct CalendarPainter<'a> {
    host: &'a Host,
    list: DrawListBuilder,
    clips: usize,
    error: Option<Error>,
}

fn draw_rect(rect: Rect) -> winisland_plugin_api::sdk::Rect {
    winisland_plugin_api::sdk::Rect::new(rect.left, rect.top, rect.width(), rect.height())
}

impl CalendarPainter<'_> {
    fn measure(&mut self, text: &str, size: f32, bold: bool) -> TextMetricsV2 {
        let style = if bold {
            TextStyle::default_at(size).bold()
        } else {
            TextStyle::default_at(size)
        };
        match self
            .host
            .text()
            .and_then(|api| api.measure_style(text, &style))
        {
            Ok(metrics) => metrics,
            Err(error) => {
                self.error = Some(error);
                TextMetricsV2::default()
            }
        }
    }

    fn text(&mut self, text: &str, bounds: Rect, size: f32, bold: bool, color: Rgba) {
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        let style = if bold {
            TextStyle::default_at(size).bold()
        } else {
            TextStyle::default_at(size)
        };
        let metrics = self.measure(text, size, bold);
        let rect = winisland_plugin_api::sdk::Rect::new(
            bounds.left,
            bounds.top + (bounds.height() - metrics.height) / 2.0,
            bounds.width(),
            metrics.height.max(size),
        );
        self.list.text(text, rect, &style, color);
    }

    fn fill_rect(&mut self, rect: Rect, color: Rgba) {
        self.list.fill_rect(draw_rect(rect), color);
    }

    fn fill_circle(&mut self, center: Point, radius: f32, color: Rgba) {
        self.list.fill_circle(center.x, center.y, radius, color);
    }

    fn stroke_line(&mut self, from: Point, to: Point, width: f32, color: Rgba) {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let length = dx.hypot(dy);
        let radius = width / 2.0;
        if length <= f32::EPSILON {
            self.fill_circle(from, radius, color);
            return;
        }
        self.list.transform([
            dx / length,
            dy / length,
            -dy / length,
            dx / length,
            from.x,
            from.y,
        ]);
        self.list.fill_round_rect(
            winisland_plugin_api::sdk::Rect::new(-radius, -radius, length + width, width),
            radius,
            color,
        );
        self.list.pop_transform();
    }

    fn save(&self) -> usize {
        self.clips
    }

    fn clip_rect(&mut self, rect: Rect) {
        self.list.clip_rect(draw_rect(rect));
        self.clips += 1;
    }

    fn restore_to(&mut self, count: usize) {
        while self.clips > count {
            self.list.pop_clip();
            self.clips -= 1;
        }
    }
}

fn draw_widget_text_centered(
    painter: &mut CalendarPainter<'_>,
    text: &str,
    bounds: Rect,
    size: f32,
    bold: bool,
    color: Rgba,
) {
    let width = painter.measure(text, size, bold).width;
    painter.text(
        text,
        Rect::from_xywh(
            bounds.left + (bounds.width() - width) / 2.0,
            bounds.top,
            width + 1.0,
            bounds.height(),
        ),
        size,
        bold,
        color,
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CalendarAction {
    PreviousYear,
    PreviousMonth,
    NextMonth,
    NextYear,
    Today,
}

impl CalendarAction {
    const COUNT: usize = 5;

    fn index(self) -> usize {
        match self {
            Self::PreviousYear => 0,
            Self::PreviousMonth => 1,
            Self::NextMonth => 2,
            Self::NextYear => 3,
            Self::Today => 4,
        }
    }
}

struct CalendarLayout {
    inner: Rect,
    panel_w: f32,
    divider_x: f32,
    title: Rect,
    grid: Rect,
    buttons: [(CalendarAction, Rect); 4],
}

#[derive(Clone, Copy)]
struct Transition {
    from_offset: i32,
    direction: f32,
    vertical: bool,
    started: Instant,
}

struct Motion {
    offset: i32,
    transition: Option<Transition>,
    press: Option<(CalendarAction, Instant)>,
    pulse: Option<Instant>,
    hover_target: Option<CalendarAction>,
    hover: [f32; CalendarAction::COUNT],
    last_frame: Option<Instant>,
}

#[derive(Clone, Copy)]
struct MotionFrame {
    offset: i32,
    transition: Option<(Transition, f32)>,
    press: Option<(CalendarAction, f32)>,
    pulse: Option<f32>,
    hover: [f32; CalendarAction::COUNT],
}

struct PageText {
    key: (Region, (u16, u16, u16), (u16, u16)),
    weekday: String,
    today_caption: String,
    title: String,
    holidays: Rc<Vec<Holiday>>,
    holiday_dates: Vec<String>,
}

type HolidayCache = Vec<((Region, u16, u16), Rc<Vec<Holiday>>)>;

thread_local! {
    static MOTION: RefCell<Motion> = const { RefCell::new(Motion {
        offset: 0,
        transition: None,
        press: None,
        pulse: None,
        hover_target: None,
        hover: [0.0; CalendarAction::COUNT],
        last_frame: None,
    }) };
    static PAGE_TEXT: RefCell<Option<PageText>> = const { RefCell::new(None) };
    static HOLIDAYS: RefCell<HolidayCache> = const { RefCell::new(Vec::new()) };
}

fn progress(started: Instant, now: Instant, duration: f32) -> Option<f32> {
    let t = now.saturating_duration_since(started).as_secs_f32() / duration;
    (t < 1.0).then_some(t.max(0.0))
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_out_back(t: f32) -> f32 {
    const OVERSHOOT: f32 = 1.70158;
    let shifted = t - 1.0;
    1.0 + (OVERSHOOT + 1.0) * shifted.powi(3) + OVERSHOOT * shifted.powi(2)
}

fn apply_action(action: CalendarAction) {
    MOTION.with(|motion| {
        let mut motion = motion.borrow_mut();
        let now = Instant::now();
        let previous = motion.offset;
        let next = match action {
            CalendarAction::PreviousYear => previous - 12,
            CalendarAction::PreviousMonth => previous - 1,
            CalendarAction::NextMonth => previous + 1,
            CalendarAction::NextYear => previous + 12,
            CalendarAction::Today => 0,
        };
        motion.offset = next;
        motion.press = Some((action, now));
        if next != previous {
            let delta = next - previous;
            motion.transition = Some(Transition {
                from_offset: previous,
                direction: delta.signum() as f32,
                vertical: delta % 12 == 0,
                started: now,
            });
        } else if action == CalendarAction::Today {
            motion.pulse = Some(now);
        }
    });
}

fn reset_to_today() {
    MOTION.with(|motion| {
        let mut motion = motion.borrow_mut();
        motion.offset = 0;
        motion.transition = None;
        motion.press = None;
        motion.pulse = None;
        motion.hover_target = None;
        motion.hover = [0.0; CalendarAction::COUNT];
    });
}

fn set_hover(action: Option<CalendarAction>) -> bool {
    MOTION.with(|motion| {
        let mut motion = motion.borrow_mut();
        let changed = motion.hover_target != action;
        motion.hover_target = action;
        changed
    })
}

fn is_animating() -> bool {
    MOTION.with(|motion| {
        let motion = motion.borrow();
        let now = Instant::now();
        motion
            .transition
            .is_some_and(|transition| progress(transition.started, now, TRANSITION_SECS).is_some())
            || motion
                .press
                .is_some_and(|(_, started)| progress(started, now, PRESS_SECS).is_some())
            || motion
                .pulse
                .is_some_and(|started| progress(started, now, PULSE_SECS).is_some())
            || motion.hover.iter().enumerate().any(|(index, value)| {
                let target = f32::from(
                    motion
                        .hover_target
                        .is_some_and(|action| action.index() == index),
                );
                (target - value).abs() > 0.005
            })
    })
}

fn advance_motion() -> MotionFrame {
    MOTION.with(|motion| {
        let mut motion = motion.borrow_mut();
        let now = Instant::now();
        let dt = motion
            .last_frame
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .min(0.1);
        motion.last_frame = Some(now);
        let blend = 1.0 - (-dt * HOVER_RATE).exp();
        let hover_target = motion.hover_target;
        for (index, value) in motion.hover.iter_mut().enumerate() {
            let target = f32::from(hover_target.is_some_and(|action| action.index() == index));
            *value += (target - *value) * blend;
            if (target - *value).abs() <= 0.005 {
                *value = target;
            }
        }
        let transition = motion.transition.and_then(|transition| {
            progress(transition.started, now, TRANSITION_SECS)
                .map(|t| (transition, ease_out_cubic(t)))
        });
        if transition.is_none() {
            motion.transition = None;
        }
        let press = motion
            .press
            .and_then(|(action, started)| progress(started, now, PRESS_SECS).map(|t| (action, t)));
        if press.is_none() {
            motion.press = None;
        }
        let pulse = motion
            .pulse
            .and_then(|started| progress(started, now, PULSE_SECS));
        if pulse.is_none() {
            motion.pulse = None;
        }
        MotionFrame {
            offset: motion.offset,
            transition,
            press,
            pulse,
            hover: motion.hover,
        }
    })
}

fn month_at(year: u16, month: u16, offset: i32) -> (u16, u16) {
    let index = i32::from(year) * 12 + i32::from(month) - 1 + offset;
    (
        index.div_euclid(12).clamp(1601, 9999) as u16,
        index.rem_euclid(12) as u16 + 1,
    )
}

fn json_number(args: &str, key: &str) -> Option<u16> {
    let rest = &args[args.find(&format!("\"{key}\""))? + key.len() + 2..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn month_holidays(host: &Host, args: &str) -> Result<String, Error> {
    let system = host.system()?;
    let today = system.local_datetime()?;
    let year = json_number(args, "year")
        .filter(|year| (1601..=9999).contains(year))
        .unwrap_or(today.year);
    let month = json_number(args, "month")
        .filter(|month| (1..=12).contains(month))
        .unwrap_or(today.month);
    let region = Region::from_lang(&system.current_language()?);
    let holidays = holidays_for(&system, region, year, month)?;
    if holidays.is_empty() {
        return Ok(format!("{year}-{month:02}: no holidays"));
    }
    let days: Vec<String> = holidays
        .iter()
        .map(|holiday| format!("{year}-{month:02}-{:02} {}", holiday.day, holiday.name))
        .collect();
    Ok(days.join("; "))
}

fn holidays_for(
    system: &SystemApi,
    region: Region,
    year: u16,
    month: u16,
) -> Result<Rc<Vec<Holiday>>, Error> {
    HOLIDAYS.with(|cell| {
        let mut cache = cell.borrow_mut();
        let key = (region, year, month);
        if let Some((_, holidays)) = cache.iter().find(|(entry, _)| *entry == key) {
            return Ok(holidays.clone());
        }
        let mut failure = None;
        let holidays = Rc::new(region.holidays(year, month, |y, m, d| {
            if failure.is_some() {
                return None;
            }
            match system.lunar_date(y, m, d) {
                Ok(date) => date,
                Err(error) => {
                    failure = Some(error);
                    None
                }
            }
        }));
        if let Some(error) = failure {
            return Err(error);
        }
        if cache.len() >= HOLIDAY_CACHE_LIMIT {
            cache.remove(0);
        }
        cache.push((key, holidays.clone()));
        Ok(holidays)
    })
}

fn layout(ox: f32, oy: f32, w: f32, h: f32, scale: f32) -> CalendarLayout {
    let radius = (48.0 * scale).min(w / 2.0).min(h / 2.0).max(0.0);
    let inset = (radius * (1.0 - std::f32::consts::FRAC_1_SQRT_2) + 8.0 * scale).max(24.0 * scale);
    let (inner_x, inner_y) = (ox + inset, oy + inset);
    let inset_x = inner_x - ox;
    let inset_y = inner_y - oy;
    let inner = Rect::from_xywh(
        inner_x,
        inner_y,
        (w - inset_x * 2.0).max(0.0),
        (h - inset_y * 2.0).max(0.0),
    );
    let panel_w = inner.width() * PANEL_RATIO;
    let divider_x = inner.left + panel_w + DIVIDER_GAP * scale / 2.0;
    let column_left = divider_x + DIVIDER_GAP * scale / 2.0;
    let column_w = (inner.right - column_left).max(0.0);
    let header_h = (inner.height() * 0.15).clamp(16.0 * scale, 22.0 * scale);
    let button = header_h;
    let actions = [
        CalendarAction::PreviousYear,
        CalendarAction::PreviousMonth,
        CalendarAction::NextMonth,
        CalendarAction::NextYear,
    ];
    let buttons_left = inner.right - button * actions.len() as f32;
    let buttons = std::array::from_fn(|index| {
        (
            actions[index],
            Rect::from_xywh(
                buttons_left + button * index as f32,
                inner.top,
                button,
                button,
            ),
        )
    });
    let grid_top = inner.top + header_h + HEADER_GAP * scale;
    CalendarLayout {
        inner,
        panel_w,
        divider_x,
        title: Rect::from_xywh(
            column_left,
            inner.top,
            (buttons_left - column_left).max(0.0),
            header_h,
        ),
        grid: Rect::from_xywh(
            column_left,
            grid_top,
            column_w,
            (inner.bottom - grid_top).max(0.0),
        ),
        buttons,
    }
}

fn draw_text_left(
    painter: &mut CalendarPainter<'_>,
    text: &str,
    x: f32,
    bounds: Rect,
    size: f32,
    bold: bool,
    color: Rgba,
) -> f32 {
    let width = painter.measure(text, size, bold).width;
    painter.text(
        text,
        Rect::from_xywh(x, bounds.top, width + 1.0, bounds.height()),
        size,
        bold,
        color,
    );
    width
}

fn fitted_size(
    painter: &mut CalendarPainter<'_>,
    text: &str,
    size: f32,
    bold: bool,
    max_width: f32,
    min_size: f32,
) -> f32 {
    let width = painter.measure(text, size, bold).width;
    if width <= max_width || width <= f32::EPSILON {
        size
    } else {
        (size * max_width / width).max(min_size)
    }
}

fn truncated(painter: &mut CalendarPainter<'_>, text: &str, size: f32, max_width: f32) -> String {
    if painter.measure(text, size, false).width <= max_width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if painter.measure(&candidate, size, false).width <= max_width {
            return candidate;
        }
    }
    String::new()
}

fn draw_chevrons(
    painter: &mut CalendarPainter<'_>,
    rect: Rect,
    count: usize,
    left: bool,
    color: Rgba,
    scale: f32,
) {
    let arm = rect.height() * 0.16;
    let spacing = arm * 1.1;
    let direction = if left { -1.0 } else { 1.0 };
    let start = rect.center_x() - spacing * (count as f32 - 1.0) / 2.0;
    for index in 0..count {
        let tip_x = start + spacing * index as f32 + direction * arm / 2.0;
        let back_x = tip_x - direction * arm;
        for sign in [-1.0, 1.0] {
            painter.stroke_line(
                Point::new(back_x, rect.center_y() + sign * arm),
                Point::new(tip_x, rect.center_y()),
                1.5 * scale,
                color,
            );
        }
    }
}

fn slide(transition: Option<(Transition, f32)>, distance: Vec2, incoming: bool) -> (Vec2, f32) {
    let Some((transition, eased)) = transition else {
        return (Vec2::new(0.0, 0.0), 1.0);
    };
    let travel = if transition.vertical {
        Vec2::new(0.0, distance.y)
    } else {
        Vec2::new(distance.x, 0.0)
    };
    let amount = if incoming {
        (1.0 - eased) * transition.direction
    } else {
        -eased * transition.direction
    };
    (
        Vec2::new(travel.x * amount, travel.y * amount),
        if incoming { eased } else { 1.0 - eased },
    )
}

struct DayStyle {
    region: Region,
    today_key: (u16, u16, u16),
    alpha: u8,
    text_color: Rgba,
    scale: f32,
    today_scale: f32,
}

fn draw_days(
    painter: &mut CalendarPainter<'_>,
    area: Rect,
    year: u16,
    month: u16,
    holidays: &[Holiday],
    style: &DayStyle,
    opacity: f32,
) {
    if opacity <= 0.01 {
        return;
    }
    let alpha = style.alpha as f32 * opacity;
    let fade = |factor: f32| style.text_color.with_alpha((alpha * factor) as u8);
    let week_start = style.region.week_start();
    let days = days_in_month(year, month);
    let first_column = (weekday(year, month, 1) + 7 - week_start) % 7;
    let rows = (first_column + days).div_ceil(7).max(1);
    let cell_w = area.width() / 7.0;
    let cell_h = area.height() / f32::from(rows);
    let label_size = (cell_h * 0.5).clamp(8.0 * style.scale, 12.0 * style.scale);
    for day in 1..=days {
        let index = first_column + day - 1;
        let column = index % 7;
        let cell = Rect::from_xywh(
            area.left + f32::from(column) * cell_w,
            area.top + f32::from(index / 7) * cell_h,
            cell_w,
            cell_h,
        );
        let label = DAY_LABELS[usize::from(day - 1)];
        let is_holiday = holidays.iter().any(|holiday| holiday.day == day);
        if (year, month, day) == style.today_key {
            painter.fill_circle(
                Point::new(cell.center_x(), cell.center_y()),
                cell_w.min(cell_h) * 0.46 * style.today_scale.max(0.0),
                ACCENT.with_alpha(alpha as u8),
            );
            draw_widget_text_centered(
                painter,
                label,
                cell,
                label_size,
                true,
                Rgba::WHITE.with_alpha(alpha as u8),
            );
            continue;
        }
        let weekend = matches!((column + week_start) % 7, 0 | 6);
        let past = (year, month, day) < style.today_key;
        let emphasis: f32 = match (weekend, past) {
            (false, false) => 0.92,
            (false, true) => 0.55,
            (true, false) => 0.5,
            (true, true) => 0.32,
        };
        draw_widget_text_centered(
            painter,
            label,
            cell,
            label_size,
            is_holiday,
            if is_holiday {
                ACCENT.with_alpha((alpha * emphasis.max(0.6)) as u8)
            } else {
                fade(emphasis)
            },
        );
        if is_holiday {
            painter.fill_circle(
                Point::new(cell.center_x(), cell.center_y() + label_size * 0.72),
                1.4 * style.scale,
                ACCENT.with_alpha((alpha * 0.9) as u8),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_calendar_page(
    painter: &mut CalendarPainter<'_>,
    ox: f32,
    oy: f32,
    w: f32,
    h: f32,
    alpha: u8,
    scale: f32,
    text_color: Rgba,
) -> Result<(), Error> {
    if alpha <= 20 {
        return Ok(());
    }
    let motion = advance_motion();
    let system = painter.host.system()?;
    let today = system.local_datetime()?;
    let region = Region::from_lang(&system.current_language()?);
    let (year, month) = month_at(today.year, today.month, motion.offset);
    let is_current_month = year == today.year && month == today.month;
    let previous_month = motion
        .transition
        .map(|(transition, _)| month_at(today.year, today.month, transition.from_offset));
    let layout = layout(ox, oy, w, h, scale);
    let inner = layout.inner;
    let fade = |factor: f32| text_color.with_alpha((alpha as f32 * factor) as u8);

    PAGE_TEXT.with(|cell| {
        let mut cache = cell.borrow_mut();
        let key = (region, (today.year, today.month, today.day), (year, month));
        if cache.as_ref().is_none_or(|text| text.key != key) {
            let holidays = holidays_for(&system, region, year, month)?;
            let holiday_dates = holidays
                .iter()
                .map(|holiday| region.short_date(month, holiday.day))
                .collect();
            *cache = Some(PageText {
                key,
                weekday: region.weekday_name(today.day_of_week),
                today_caption: region.month_title(today.year, today.month),
                title: region.month_title(year, month),
                holidays,
                holiday_dates,
            });
        }
        let Some(text) = cache.as_ref() else {
            return Ok(());
        };

        let panel_w = layout.panel_w;
        let min_size = 7.0 * scale;
        let weekday_size = fitted_size(
            painter,
            &text.weekday,
            (inner.height() * 0.09).clamp(9.0 * scale, 12.0 * scale),
            true,
            panel_w,
            min_size,
        );
        let day_size = (inner.height() * 0.42)
            .min(panel_w * 0.55)
            .max(24.0 * scale);
        let caption_size = fitted_size(
            painter,
            &text.today_caption,
            (inner.height() * 0.085).clamp(9.0 * scale, 12.0 * scale),
            false,
            panel_w,
            min_size,
        );
        let weekday_rect = Rect::from_xywh(inner.left, inner.top, panel_w, weekday_size * 1.6);
        draw_text_left(
            painter,
            &text.weekday,
            inner.left,
            weekday_rect,
            weekday_size,
            true,
            ACCENT.with_alpha(alpha),
        );
        let day_rect = Rect::from_xywh(inner.left, weekday_rect.bottom, panel_w, day_size * 1.12);
        draw_text_left(
            painter,
            DAY_LABELS[usize::from(today.day.clamp(1, 31) - 1)],
            inner.left,
            day_rect,
            day_size,
            true,
            fade(1.0),
        );
        let caption_rect =
            Rect::from_xywh(inner.left, day_rect.bottom, panel_w, caption_size * 1.6);
        draw_text_left(
            painter,
            &text.today_caption,
            inner.left,
            caption_rect,
            caption_size,
            false,
            fade(0.6),
        );

        let list_opacity = motion.transition.map_or(1.0, |(_, eased)| eased);
        let line_size = (caption_size * 0.92).max(min_size);
        let line_h = line_size * 1.55;
        let list_top = caption_rect.bottom + 4.0 * scale;
        let capacity = ((inner.bottom - list_top) / line_h).floor().max(0.0) as usize;
        let first = if is_current_month {
            text.holidays
                .iter()
                .position(|holiday| holiday.day >= today.day)
                .unwrap_or(text.holidays.len())
                .min(text.holidays.len().saturating_sub(capacity))
        } else {
            0
        };
        let list_rise = (1.0 - list_opacity) * 4.0 * scale;
        for (line, index) in (first..text.holidays.len()).take(capacity).enumerate() {
            let row = Rect::from_xywh(
                inner.left,
                list_top + line as f32 * line_h + list_rise,
                panel_w,
                line_h,
            );
            let date_w = draw_text_left(
                painter,
                &text.holiday_dates[index],
                inner.left,
                row,
                line_size,
                true,
                ACCENT.with_alpha((alpha as f32 * 0.85 * list_opacity) as u8),
            );
            let name_x = inner.left + date_w + 5.0 * scale;
            let name = truncated(
                painter,
                text.holidays[index].name,
                line_size,
                (inner.left + panel_w - name_x).max(0.0),
            );
            draw_text_left(
                painter,
                &name,
                name_x,
                row,
                line_size,
                false,
                fade(0.78 * list_opacity),
            );
        }

        painter.fill_rect(
            Rect::from_xywh(layout.divider_x, inner.top, scale.max(1.0), inner.height()),
            fade(0.12),
        );

        let title_hover = motion.hover[CalendarAction::Today.index()];
        let title_color = |opacity: f32, current: bool| {
            if current {
                fade(opacity)
            } else {
                ACCENT.with_alpha((alpha as f32 * opacity * (1.0 - 0.35 * title_hover)) as u8)
            }
        };
        let title_travel = Vec2::new(14.0 * scale, layout.title.height() * 0.6);
        let save_count = painter.save();
        painter.clip_rect(layout.title);
        if let Some((from_year, from_month)) = previous_month {
            let old_title = region.month_title(from_year, from_month);
            let (shift, opacity) = slide(motion.transition, title_travel, false);
            let old_current = from_year == today.year && from_month == today.month;
            let old_size = fitted_size(
                painter,
                &old_title,
                layout.title.height() * 0.62,
                true,
                layout.title.width(),
                min_size,
            );
            draw_text_left(
                painter,
                &old_title,
                layout.title.left + shift.x,
                layout.title.offset(Vec2::new(0.0, shift.y)),
                old_size,
                true,
                title_color(opacity, old_current),
            );
        }
        let (shift, opacity) = slide(motion.transition, title_travel, true);
        let title_size = fitted_size(
            painter,
            &text.title,
            layout.title.height() * 0.62,
            true,
            layout.title.width(),
            min_size,
        );
        draw_text_left(
            painter,
            &text.title,
            layout.title.left + shift.x,
            layout.title.offset(Vec2::new(0.0, shift.y)),
            title_size,
            true,
            title_color(opacity, is_current_month),
        );
        painter.restore_to(save_count);

        for (action, rect) in layout.buttons {
            let (count, left) = match action {
                CalendarAction::PreviousYear => (2, true),
                CalendarAction::PreviousMonth => (1, true),
                CalendarAction::NextMonth => (1, false),
                _ => (2, false),
            };
            let hover = motion.hover[action.index()];
            let pressed = motion
                .press
                .filter(|(pressed, _)| *pressed == action)
                .map_or(0.0, |(_, t)| 1.0 - ease_out_cubic(t));
            let highlight = 0.1 * hover + 0.22 * pressed;
            if highlight > 0.005 {
                painter.fill_circle(
                    Point::new(rect.center_x(), rect.center_y()),
                    rect.height() * 0.44,
                    fade(highlight),
                );
            }
            let icon_scale = 1.0 - 0.18 * pressed;
            let icon = Rect::from_xywh(
                rect.center_x() - rect.width() * icon_scale / 2.0,
                rect.center_y() - rect.height() * icon_scale / 2.0,
                rect.width() * icon_scale,
                rect.height() * icon_scale,
            );
            draw_chevrons(
                painter,
                icon,
                count,
                left,
                fade(0.7 + 0.3 * hover.max(pressed)),
                scale,
            );
        }

        let grid = layout.grid;
        let week_start = region.week_start();
        let current_rows =
            (weekday(year, month, 1) + 7 - week_start) % 7 + days_in_month(year, month);
        let header_h = grid.height() / f32::from(current_rows.div_ceil(7) + 1);
        let header_size = (header_h * 0.5).clamp(8.0 * scale, 12.0 * scale) * 0.85;
        for column in 0..7u16 {
            let weekend = matches!((column + week_start) % 7, 0 | 6);
            draw_widget_text_centered(
                painter,
                region.weekday_initial(column + week_start),
                Rect::from_xywh(
                    grid.left + f32::from(column) * grid.width() / 7.0,
                    grid.top,
                    grid.width() / 7.0,
                    header_h,
                ),
                header_size,
                true,
                fade(if weekend { 0.3 } else { 0.45 }),
            );
        }

        let days_area = Rect::from_xywh(
            grid.left,
            grid.top + header_h,
            grid.width(),
            (grid.height() - header_h).max(0.0),
        );
        let lands_on_today = motion.transition.is_some() && is_current_month;
        let today_scale = if let Some((_, eased)) = motion.transition.filter(|_| lands_on_today) {
            ease_out_back(eased)
        } else if let Some(t) = motion.pulse {
            1.0 + 0.18 * (t * std::f32::consts::PI).sin()
        } else {
            1.0
        };
        let style = DayStyle {
            region,
            today_key: (today.year, today.month, today.day),
            alpha,
            text_color,
            scale,
            today_scale,
        };
        let days_travel = Vec2::new(days_area.width() * 0.3, days_area.height() * 0.3);
        let save_count = painter.save();
        painter.clip_rect(days_area);
        if let Some((from_year, from_month)) = previous_month {
            let (shift, opacity) = slide(motion.transition, days_travel, false);
            let old_holidays = holidays_for(&system, region, from_year, from_month)?;
            let old_style = DayStyle {
                today_scale: 1.0,
                ..style
            };
            draw_days(
                painter,
                days_area.offset(shift),
                from_year,
                from_month,
                &old_holidays,
                &old_style,
                opacity,
            );
        }
        let (shift, opacity) = slide(motion.transition, days_travel, true);
        draw_days(
            painter,
            days_area.offset(shift),
            year,
            month,
            &text.holidays,
            &style,
            opacity,
        );
        painter.restore_to(save_count);
        Ok(())
    })
}
