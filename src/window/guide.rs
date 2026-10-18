use std::cell::{OnceCell, RefCell};
use std::time::{Duration, Instant};

use winisland_core::i18n::tr;
use winisland_platform::{
    CursorKind, InputState, Key, LogicalWindowSize, MouseButton, PlatformEvent, SettingsSpec,
    Theme, WindowId,
};
use winisland_render::FontWeight;
use winisland_render::text::FontManager;
use winisland_render::{
    FontStyle, Image, ImageOptions, Painter, Point, Radius, Rect, Renderer, RendererTargetId, Rgba,
    Sampling, StrokeCap, Vec2,
};

use crate::platform::{MonitorRef, WindowRef, window};
use crate::utils::color::{SettingsTheme, dark_settings_theme, light_settings_theme};
use crate::utils::settings_ui::chrome::{
    SidebarRow, draw_sidebar_row, draw_traffic_lights, draw_window_base, sidebar_panel,
    traffic_light_at, traffic_lights_hovered,
};
use crate::utils::settings_ui::items::{
    CONTENT_PADDING, GROUP_INNER_PAD, GROUP_RADIUS, POPUP_BTN_H, POPUP_BTN_R, POPUP_BTN_W,
    SIDEBAR_PAD,
};
use crate::utils::settings_ui::text_baseline;
use crate::utils::settings_ui::{SettingsPainter, ellipsize_text, settings_color};
use crate::window::settings::{SETTINGS_HEADER_H, SIDEBAR_ROW_GAP, SIDEBAR_ROW_H, SIDEBAR_W};

const WIN_W: f32 = 720.0;
const GUIDE_TAB_TOP: f32 = 52.0;
const WIN_H: f32 = 460.0;
const GUIDE_TITLE: &str = "WinIsland Guide";
const FEATURE_ROW_H: f32 = 56.0;
const FEATURE_TILE: f32 = 28.0;
const FOOTER_H: f32 = 60.0;
const KEYCAP_H: f32 = 24.0;
const TRANSITION_SECS: f32 = 0.24;
const HOVER_RATE: f32 = 18.0;
const FRAME_INTERVAL: Duration = Duration::from_millis(8);
const WELCOME_ICON_SIZE: f32 = 64.0;

thread_local! {
    static GUIDE_ICON: OnceCell<Option<Image>> = const { OnceCell::new() };
}

fn app_icon() -> Option<Image> {
    GUIDE_ICON.with(|icon| {
        icon.get_or_init(|| Image::from_encoded(include_bytes!("../../resources/icon-dark.png")))
            .clone()
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    Welcome,
    Basics,
    Gestures,
    Shortcuts,
    Pages,
    Music,
    Widgets,
    GesturesShortcuts,
    Expand,
    Collapse,
    SwipeUp,
    SwipeSide,
    RightDrag,
    DropInstall,
    HideShortcut,
    Fullscreen,
    CopiedLink,
    SwitchPages,
    BuiltinPages,
    Customize,
}

const ICON_COUNT: usize = 20;

impl Icon {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Welcome => include_bytes!("../../resources/in_app/guide/welcome.png"),
            Self::Basics => include_bytes!("../../resources/in_app/guide/basics.png"),
            Self::Gestures => include_bytes!("../../resources/in_app/guide/gestures.png"),
            Self::Shortcuts => include_bytes!("../../resources/in_app/guide/shortcuts.png"),
            Self::Pages => include_bytes!("../../resources/in_app/guide/pages.png"),
            Self::Music => include_bytes!("../../resources/in_app/settings/music.png"),
            Self::Widgets => include_bytes!("../../resources/in_app/settings/widget.png"),
            Self::GesturesShortcuts => {
                include_bytes!("../../resources/in_app/guide/gestures_shortcuts.png")
            }
            Self::Expand => include_bytes!("../../resources/in_app/guide/expand.png"),
            Self::Collapse => include_bytes!("../../resources/in_app/guide/collapse.png"),
            Self::SwipeUp => include_bytes!("../../resources/in_app/guide/swipe_up.png"),
            Self::SwipeSide => include_bytes!("../../resources/in_app/guide/swipe_side.png"),
            Self::RightDrag => include_bytes!("../../resources/in_app/guide/right_drag.png"),
            Self::DropInstall => include_bytes!("../../resources/in_app/guide/drop_install.png"),
            Self::HideShortcut => include_bytes!("../../resources/in_app/guide/hide_shortcut.png"),
            Self::Fullscreen => include_bytes!("../../resources/in_app/guide/fullscreen.png"),
            Self::CopiedLink => include_bytes!("../../resources/in_app/guide/copied_link.png"),
            Self::SwitchPages => include_bytes!("../../resources/in_app/guide/switch_pages.png"),
            Self::BuiltinPages => include_bytes!("../../resources/in_app/guide/builtin_pages.png"),
            Self::Customize => include_bytes!("../../resources/in_app/guide/customize.png"),
        }
    }

    fn image(self) -> Option<Image> {
        ICON_IMAGES.with(|images| {
            images.borrow_mut()[self as usize]
                .get_or_insert_with(|| Image::from_encoded(self.bytes()))
                .clone()
        })
    }
}

thread_local! {
    static ICON_IMAGES: RefCell<[Option<Option<Image>>; ICON_COUNT]> = const { RefCell::new([const { None }; ICON_COUNT]) };
}

struct Feature {
    icon: Icon,
    title: &'static str,
    detail: &'static str,
    keycap: Option<&'static str>,
}

struct GuidePage {
    icon: Icon,
    tab: &'static str,
    title: &'static str,
    subtitle: &'static str,
    features: &'static [Feature],
}

const fn feature(icon: Icon, title: &'static str, detail: &'static str) -> Feature {
    Feature {
        icon,
        title,
        detail,
        keycap: None,
    }
}

const PAGES: [GuidePage; 5] = [
    GuidePage {
        icon: Icon::Welcome,
        tab: "guide_tab_welcome",
        title: "guide_welcome_title",
        subtitle: "guide_welcome_subtitle",
        features: &[
            feature(
                Icon::Music,
                "guide_welcome_music",
                "guide_welcome_music_detail",
            ),
            feature(
                Icon::Widgets,
                "guide_welcome_pages",
                "guide_welcome_pages_detail",
            ),
            feature(
                Icon::GesturesShortcuts,
                "guide_welcome_gestures",
                "guide_welcome_gestures_detail",
            ),
        ],
    },
    GuidePage {
        icon: Icon::Basics,
        tab: "guide_basics_title",
        title: "guide_basics_title",
        subtitle: "guide_basics_subtitle",
        features: &[
            feature(
                Icon::Expand,
                "guide_basics_expand",
                "guide_basics_expand_detail",
            ),
            feature(
                Icon::Collapse,
                "guide_basics_collapse",
                "guide_basics_collapse_detail",
            ),
            feature(
                Icon::Music,
                "guide_basics_music",
                "guide_basics_music_detail",
            ),
        ],
    },
    GuidePage {
        icon: Icon::Gestures,
        tab: "guide_gestures_title",
        title: "guide_gestures_title",
        subtitle: "guide_gestures_subtitle",
        features: &[
            feature(
                Icon::SwipeUp,
                "guide_gestures_hide",
                "guide_gestures_hide_detail",
            ),
            feature(
                Icon::SwipeSide,
                "guide_gestures_side",
                "guide_gestures_side_detail",
            ),
            feature(
                Icon::RightDrag,
                "guide_gestures_move",
                "guide_gestures_move_detail",
            ),
            feature(
                Icon::DropInstall,
                "guide_gestures_plugin",
                "guide_gestures_plugin_detail",
            ),
        ],
    },
    GuidePage {
        icon: Icon::Shortcuts,
        tab: "guide_shortcuts_title",
        title: "guide_shortcuts_title",
        subtitle: "guide_shortcuts_subtitle",
        features: &[
            Feature {
                icon: Icon::HideShortcut,
                title: "guide_shortcuts_hide",
                detail: "guide_shortcuts_hide_detail",
                keycap: Some("Ctrl + Alt + H"),
            },
            Feature {
                icon: Icon::Fullscreen,
                title: "guide_shortcuts_fullscreen",
                detail: "guide_shortcuts_fullscreen_detail",
                keycap: Some("Ctrl + Alt + H"),
            },
            feature(
                Icon::CopiedLink,
                "guide_shortcuts_link",
                "guide_shortcuts_link_detail",
            ),
        ],
    },
    GuidePage {
        icon: Icon::Pages,
        tab: "guide_pages_title",
        title: "guide_pages_title",
        subtitle: "guide_pages_subtitle",
        features: &[
            feature(
                Icon::SwitchPages,
                "guide_pages_switch",
                "guide_pages_switch_detail",
            ),
            feature(
                Icon::BuiltinPages,
                "guide_pages_list",
                "guide_pages_list_detail",
            ),
            feature(
                Icon::Customize,
                "guide_pages_customize",
                "guide_pages_customize_detail",
            ),
        ],
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Close,
    Minimize,
    Tab(usize),
    Back,
    Next,
}

pub(crate) struct GuideApp {
    window: Option<WindowRef>,
    renderer_target: Option<RendererTargetId>,
    theme_setting: String,
    is_light: bool,
    focused: bool,
    page: usize,
    transition: Option<(f32, Instant)>,
    last_frame: Instant,
    mouse: (f32, f32),
    hover: Option<Hit>,
    tab_hover: [f32; PAGES.len()],
    finished: bool,
    backdrop: bool,
}

fn fade(color: Rgba, alpha: f32) -> Rgba {
    color.with_alpha_f(f32::from(color.a()) / 255.0 * alpha)
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn draw_icon(painter: Painter<'_>, icon: Icon, rect: Rect, alpha: f32) {
    if let Some(image) = icon.image() {
        crate::utils::settings_ui::chrome::draw_icon_shadow(painter, rect, alpha);
        painter.draw_image(
            &image,
            rect,
            &ImageOptions::default()
                .with_sampling(Sampling::LinearLinear)
                .with_alpha_f(alpha),
        );
    }
}

fn tab_rect(index: usize) -> Rect {
    Rect::from_xywh(
        SIDEBAR_PAD,
        GUIDE_TAB_TOP + index as f32 * (SIDEBAR_ROW_H + SIDEBAR_ROW_GAP),
        SIDEBAR_W - SIDEBAR_PAD * 2.0,
        SIDEBAR_ROW_H,
    )
}

fn next_rect() -> Rect {
    Rect::from_xywh(
        WIN_W - CONTENT_PADDING - POPUP_BTN_W,
        WIN_H - FOOTER_H / 2.0 - POPUP_BTN_H / 2.0,
        POPUP_BTN_W,
        POPUP_BTN_H,
    )
}

fn back_rect() -> Rect {
    next_rect().offset(Vec2::new(-(POPUP_BTN_W + 8.0), 0.0))
}

fn draw_pill_btn(
    painter: Painter<'_>,
    rect: Rect,
    label: &str,
    text_color: Rgba,
    bg_color: Rgba,
    border_color: Rgba,
) {
    painter.fill_round_rect(rect, Radius::uniform(POPUP_BTN_R), bg_color);
    painter.stroke_round_rect(
        Rect::from_xywh(
            rect.left + 0.375,
            rect.top + 0.375,
            rect.width() - 0.75,
            rect.height() - 0.75,
        ),
        Radius::uniform(POPUP_BTN_R),
        0.75,
        border_color,
    );
    SettingsPainter::new(painter).centered_text(
        label,
        (rect.center_x(), rect.center_y() + 4.0),
        12.0,
        FontWeight::REGULAR,
        text_color,
    );
}

impl GuideApp {
    pub(crate) fn new(theme_setting: String) -> Self {
        Self {
            window: None,
            renderer_target: None,
            theme_setting,
            is_light: false,
            focused: true,
            page: 0,
            transition: None,
            last_frame: Instant::now(),
            mouse: (-1.0, -1.0),
            hover: None,
            tab_hover: [0.0; PAGES.len()],
            finished: false,
            backdrop: false,
        }
    }

    pub(crate) fn create_window(
        &mut self,
        renderer: &mut Renderer,
        monitor: Option<MonitorRef>,
    ) -> Result<(), String> {
        let id = window()
            .create_settings(SettingsSpec {
                title: GUIDE_TITLE,
                logical_size: LogicalWindowSize::new(f64::from(WIN_W), f64::from(WIN_H)),
                monitor: monitor.as_ref().map(MonitorRef::id),
                resizable: false,
            })
            .map_err(|error| error.to_string())?;
        self.window = Some(WindowRef(id));
        self.backdrop = window().start_window_backdrop(id).is_ok();
        self.update_theme();
        self.transition = Some((1.0, Instant::now()));
        self.recreate_renderer_target(renderer)
    }

    pub(crate) fn recreate_renderer_target(
        &mut self,
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        let Some(window_ref) = self.window else {
            return Ok(());
        };
        let size = window_ref.inner_size();
        let surface = window()
            .native_surface(window_ref.id())
            .ok_or_else(|| "Guide window has no native surface".to_string())?;
        self.renderer_target = Some(
            renderer
                .create_target(surface, size.width, size.height)
                .map_err(|error| error.to_string())?,
        );
        window_ref.request_redraw();
        Ok(())
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.map(WindowRef::id)
    }

    pub(crate) fn close_requested(&self) -> bool {
        self.finished
    }

    pub(crate) fn bring_to_front(&self) {
        if let Some(window_ref) = self.window {
            window_ref.set_minimized(false);
            let _ = crate::platform::display().bring_foreign_window_to_front(GUIDE_TITLE);
            window_ref.request_redraw();
        }
    }

    pub(crate) fn close(&mut self) -> Option<RendererTargetId> {
        let target = self.renderer_target.take();
        if let Some(window_ref) = self.window.take() {
            window().destroy_window(window_ref.id());
        }
        target
    }

    fn update_theme(&mut self) {
        self.is_light = match self.theme_setting.as_str() {
            "light" => true,
            "dark" => false,
            _ => self
                .window
                .is_some_and(|window_ref| window().theme(window_ref.id()) == Some(Theme::Light)),
        };
    }

    fn theme(&self) -> SettingsTheme {
        if self.is_light {
            light_settings_theme()
        } else {
            dark_settings_theme()
        }
    }

    fn scale(&self) -> f32 {
        self.window
            .map_or(1.0, |window_ref| window_ref.scale_factor() as f32)
            .max(0.1)
    }

    fn request_redraw(&self) {
        if let Some(window_ref) = self.window {
            window_ref.request_redraw();
        }
    }

    fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        let point = Point::new(x, y);
        match traffic_light_at(x, y) {
            Some(0) => return Some(Hit::Close),
            Some(1) => return Some(Hit::Minimize),
            _ => {}
        }
        if let Some(index) = (0..PAGES.len()).find(|index| tab_rect(*index).contains(point)) {
            return Some(Hit::Tab(index));
        }
        if next_rect().contains(point) {
            return Some(Hit::Next);
        }
        if self.page > 0 && back_rect().contains(point) {
            return Some(Hit::Back);
        }
        None
    }

    fn controls_hovered(&self) -> bool {
        self.focused && traffic_lights_hovered(self.mouse.0, self.mouse.1)
    }

    fn go_to(&mut self, page: usize) {
        if page == self.page || page >= PAGES.len() {
            return;
        }
        let direction = if page > self.page { 1.0 } else { -1.0 };
        self.transition = Some((direction, Instant::now()));
        self.page = page;
        self.request_redraw();
    }

    fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Close => self.finished = true,
            Hit::Minimize => {
                if let Some(window_ref) = self.window {
                    window_ref.set_minimized(true);
                }
            }
            Hit::Tab(index) => self.go_to(index),
            Hit::Back => self.go_to(self.page.saturating_sub(1)),
            Hit::Next if self.page + 1 < PAGES.len() => self.go_to(self.page + 1),
            Hit::Next => self.finished = true,
        }
    }

    pub(crate) fn handle_window_event(&mut self, event: PlatformEvent, renderer: &mut Renderer) {
        match event {
            PlatformEvent::CloseRequested { .. } | PlatformEvent::Destroyed { .. } => {
                self.finished = true;
            }
            PlatformEvent::ThemeChanged { .. } => {
                self.update_theme();
                self.request_redraw();
            }
            PlatformEvent::Focused { focused, .. } => {
                self.focused = focused;
                self.request_redraw();
            }
            PlatformEvent::Resized { size, .. } if size.width > 0 && size.height > 0 => {
                if let Some(target) = self.renderer_target
                    && let Err(error) = renderer.resize(target, size.width, size.height)
                {
                    log::warn!("Guide renderer resize failed: {error}");
                }
                self.request_redraw();
            }
            PlatformEvent::ScaleFactorChanged { .. } => self.request_redraw(),
            PlatformEvent::CursorMoved { position, .. } => {
                let scale = self.scale();
                self.mouse = (position.x as f32 / scale, position.y as f32 / scale);
                let hover = self.hit_test(self.mouse.0, self.mouse.1);
                if hover != self.hover {
                    self.hover = hover;
                    if let Some(window_ref) = self.window {
                        window().set_cursor(
                            window_ref.id(),
                            if matches!(hover, Some(Hit::Back | Hit::Next)) {
                                CursorKind::Pointer
                            } else {
                                CursorKind::Default
                            },
                        );
                    }
                }
                self.request_redraw();
            }
            PlatformEvent::CursorLeft { .. } => {
                self.hover = None;
                self.mouse = (-1.0, -1.0);
                self.request_redraw();
            }
            PlatformEvent::MouseInput {
                state: InputState::Pressed,
                button: MouseButton::Left,
                ..
            } => match self.hit_test(self.mouse.0, self.mouse.1) {
                Some(hit) => self.activate(hit),
                None => {
                    if let Some(window_ref) = self.window {
                        window().begin_drag(window_ref.id());
                    }
                }
            },
            PlatformEvent::KeyInput {
                key,
                state: InputState::Pressed,
                ..
            } => match key {
                Key::Enter | Key::ArrowRight => self.activate(Hit::Next),
                Key::ArrowLeft => self.activate(Hit::Back),
                Key::Escape => self.finished = true,
                _ => {}
            },
            PlatformEvent::RedrawRequested { .. } => self.render(renderer),
            _ => {}
        }
    }

    fn tab_hover_target(&self, index: usize) -> f32 {
        f32::from(index != self.page && self.hover == Some(Hit::Tab(index)))
    }

    fn animating(&self) -> bool {
        self.transition
            .is_some_and(|(_, started)| started.elapsed().as_secs_f32() < TRANSITION_SECS)
            || self
                .tab_hover
                .iter()
                .enumerate()
                .any(|(index, value)| (value - self.tab_hover_target(index)).abs() > 0.005)
    }

    pub(crate) fn update(&mut self) -> Option<Instant> {
        if self.window.is_none() || !self.animating() {
            return None;
        }
        self.request_redraw();
        Some(Instant::now() + FRAME_INTERVAL)
    }

    fn advance(&mut self) {
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_frame)
            .as_secs_f32()
            .min(0.1);
        self.last_frame = now;
        let blend = 1.0 - (-dt * HOVER_RATE).exp();
        for index in 0..PAGES.len() {
            let target = self.tab_hover_target(index);
            let value = &mut self.tab_hover[index];
            *value += (target - *value) * blend;
            if (target - *value).abs() <= 0.005 {
                *value = target;
            }
        }
        if self
            .transition
            .is_some_and(|(_, started)| started.elapsed().as_secs_f32() >= TRANSITION_SECS)
        {
            self.transition = None;
        }
    }

    fn render(&mut self, renderer: &mut Renderer) {
        let Some(target) = self.renderer_target else {
            return;
        };
        self.advance();
        let scale = self.scale();
        let theme = self.theme();
        let panel = sidebar_panel(SIDEBAR_W, WIN_H);
        if self.backdrop
            && let Some(window_ref) = self.window
        {
            self.backdrop = window().update_window_backdrop(
                window_ref.id(),
                [
                    panel.left * scale,
                    panel.top * scale,
                    panel.width() * scale,
                    panel.height() * scale,
                ],
                crate::utils::settings_ui::chrome::BACKDROP_CORNER * scale,
            );
        }
        let backdrop = self.backdrop;
        let result = renderer.frame(target, |_, painter| {
            painter.scale(Vec2::new(scale, scale));
            let win_rect = Rect::from_xywh(0.0, 0.0, WIN_W, WIN_H);
            painter.save();
            crate::utils::settings_ui::chrome::clip_window(painter, win_rect);
            draw_window_base(painter, &theme, win_rect, panel, backdrop);
            self.draw_sidebar(painter, &theme);
            self.draw_header(painter, &theme);
            self.draw_content(painter, &theme);
            self.draw_footer(painter, &theme);
            painter.restore();
            crate::utils::settings_ui::chrome::stroke_window(
                painter,
                win_rect,
                settings_color(theme.separator),
            );
        });
        if let Err(error) = result {
            log::error!("Guide rendering failed: {error}");
        }
    }

    fn draw_sidebar(&self, painter: Painter<'_>, theme: &SettingsTheme) {
        draw_traffic_lights(
            painter,
            self.is_light,
            self.focused,
            self.controls_hovered(),
            theme.text_sec,
        );
        for (index, page) in PAGES.iter().enumerate() {
            let icon = page.icon.image();
            draw_sidebar_row(
                painter,
                theme,
                SidebarRow {
                    rect: tab_rect(index),
                    icon: icon.as_ref(),
                    label: &tr(page.tab),
                    selected: index == self.page,
                    focused: self.focused,
                    hover: self.tab_hover[index],
                },
            );
        }
    }

    fn draw_header(&self, painter: Painter<'_>, theme: &SettingsTheme) {
        let title_x = SIDEBAR_W + CONTENT_PADDING;
        let center_y = SETTINGS_HEADER_H / 2.0 + 2.0;
        let title = ellipsize_text(
            FontManager::global(),
            &tr(PAGES[self.page].title),
            15.0,
            FontWeight::SEMIBOLD,
            WIN_W - title_x - 70.0,
        );
        FontManager::global().draw_str(
            painter,
            &title,
            Point::new(title_x, text_baseline(center_y, 15.0)),
            15.0,
            FontWeight::SEMIBOLD,
            settings_color(theme.text_pri),
        );
        let progress = format!("{} / {}", self.page + 1, PAGES.len());
        let width = FontManager::global().measure_text_cached(&progress, 12.0, FontStyle::normal());
        FontManager::global().draw_str(
            painter,
            &progress,
            Point::new(
                WIN_W - CONTENT_PADDING - width,
                text_baseline(center_y, 12.0),
            ),
            12.0,
            FontWeight::REGULAR,
            settings_color(theme.text_sec),
        );
    }

    fn draw_content(&self, painter: Painter<'_>, theme: &SettingsTheme) {
        let (alpha, offset) = match self.transition {
            Some((direction, started)) => {
                let t = ease_out_cubic(
                    (started.elapsed().as_secs_f32() / TRANSITION_SECS).clamp(0.0, 1.0),
                );
                (t, direction * 14.0 * (1.0 - t))
            }
            None => (1.0, 0.0),
        };
        let page = &PAGES[self.page];
        let content_w = WIN_W - SIDEBAR_W - CONTENT_PADDING * 2.0;
        let save_count = painter.save();
        painter.clip_rect(Rect::from_xywh(
            SIDEBAR_W,
            SETTINGS_HEADER_H,
            WIN_W - SIDEBAR_W,
            WIN_H - SETTINGS_HEADER_H - FOOTER_H,
        ));
        painter.translate(Vec2::new(SIDEBAR_W + offset, SETTINGS_HEADER_H));

        let mut y = 8.0;
        if self.page == 0
            && let Some(icon) = app_icon()
        {
            let x = CONTENT_PADDING + (content_w - WELCOME_ICON_SIZE) / 2.0;
            painter.draw_image(
                &icon,
                Rect::from_xywh(x, y + 8.0, WELCOME_ICON_SIZE, WELCOME_ICON_SIZE),
                &ImageOptions::default()
                    .with_sampling(Sampling::LinearLinear)
                    .with_alpha_f(alpha),
            );
            y += WELCOME_ICON_SIZE + 14.0;
        }

        let subtitle = ellipsize_text(
            FontManager::global(),
            &tr(page.subtitle),
            13.0,
            FontStyle::normal(),
            content_w - 8.0,
        );
        SettingsPainter::new(painter).text(
            &subtitle,
            (CONTENT_PADDING + 4.0, y + 22.0),
            13.0,
            FontWeight::REGULAR,
            fade(settings_color(theme.text_sec), alpha),
        );
        y += 36.0;

        let group_h = page.features.len() as f32 * FEATURE_ROW_H;
        let group = Rect::from_xywh(CONTENT_PADDING, y, content_w, group_h);
        painter.fill_path(
            &winisland_render::Path::continuous_rounded_rect(group, GROUP_RADIUS),
            fade(settings_color(theme.group_bg), alpha),
        );
        painter.stroke_path(
            &winisland_render::Path::continuous_rounded_rect(
                group.inset(0.375),
                GROUP_RADIUS - 0.375,
            ),
            0.75,
            fade(settings_color(theme.group_border), alpha),
            StrokeCap::Butt,
            winisland_render::StrokeJoin::Round,
        );

        for (index, item) in page.features.iter().enumerate() {
            let row_y = y + index as f32 * FEATURE_ROW_H;
            let tile = Rect::from_xywh(
                CONTENT_PADDING + GROUP_INNER_PAD,
                row_y + (FEATURE_ROW_H - FEATURE_TILE) / 2.0,
                FEATURE_TILE,
                FEATURE_TILE,
            );
            draw_icon(painter, item.icon, tile, alpha);
            let text_x = tile.right + 12.0;
            let mut text_right = CONTENT_PADDING + content_w - GROUP_INNER_PAD;
            if let Some(keycap) = item.keycap {
                let width =
                    FontManager::global().measure_text_cached(keycap, 12.0, FontStyle::normal())
                        + 20.0;
                let cap = Rect::from_xywh(
                    text_right - width,
                    row_y + (FEATURE_ROW_H - KEYCAP_H) / 2.0,
                    width,
                    KEYCAP_H,
                );
                draw_pill_btn(
                    painter,
                    cap,
                    keycap,
                    fade(settings_color(theme.text_pri), alpha),
                    fade(settings_color(theme.control_bg), alpha),
                    fade(settings_color(theme.control_border), alpha),
                );
                text_right = cap.left - 12.0;
            }
            let text_w = (text_right - text_x).max(0.0);
            let title = ellipsize_text(
                FontManager::global(),
                &tr(item.title),
                13.0,
                FontStyle::normal(),
                text_w,
            );
            let detail = ellipsize_text(
                FontManager::global(),
                &tr(item.detail),
                11.0,
                FontStyle::normal(),
                text_w,
            );
            SettingsPainter::new(painter).text(
                &title,
                (text_x, row_y + 25.0),
                13.0,
                FontWeight::REGULAR,
                fade(settings_color(theme.text_pri), alpha),
            );
            SettingsPainter::new(painter).text(
                &detail,
                (text_x, row_y + 41.0),
                11.0,
                FontWeight::REGULAR,
                fade(settings_color(theme.text_sec), alpha),
            );
            if index + 1 < page.features.len() {
                let sep_y = row_y + FEATURE_ROW_H;
                painter.stroke_line(
                    Point::new(text_x, sep_y),
                    Point::new(CONTENT_PADDING + content_w - GROUP_INNER_PAD, sep_y),
                    0.5,
                    fade(settings_color(theme.separator), alpha),
                    StrokeCap::Butt,
                );
            }
        }
        painter.restore_to(save_count);
    }

    fn draw_footer(&self, painter: Painter<'_>, theme: &SettingsTheme) {
        if self.page > 0 {
            let hovered = self.hover == Some(Hit::Back);
            draw_pill_btn(
                painter,
                back_rect(),
                &tr("guide_back"),
                settings_color(theme.text_pri),
                settings_color(if hovered {
                    theme.control_hover
                } else {
                    theme.control_bg
                }),
                settings_color(theme.control_border),
            );
        }
        let hovered = self.hover == Some(Hit::Next);
        let accent = settings_color(theme.accent);
        let label = if self.page + 1 == PAGES.len() {
            tr("guide_get_started")
        } else {
            tr("guide_continue")
        };
        draw_pill_btn(
            painter,
            next_rect(),
            &label,
            Rgba::WHITE,
            accent.with_alpha_f(if hovered { 0.85 } else { 1.0 }),
            accent,
        );
    }
}
