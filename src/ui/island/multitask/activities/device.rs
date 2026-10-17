use std::time::{Duration, Instant};

use winisland_core::anim::{Curve, Tween};
use winisland_platform::DeviceActivity;
use winisland_render::{Painter, Point, Rect, Rgba};

use crate::ui::compact::timer::TimerContent;

use super::Activity;

const PILL_EXTENSION: f32 = 0.75;
const FADE_DURATION: Duration = Duration::from_millis(100);
const SHARE_DURATION: Duration = Duration::from_millis(420);
const TIMER_ICON_DIAMETER: f32 = 26.0;

pub(crate) struct StatusIndicators {
    camera: Tween,
    microphone: Tween,
    timer: Tween,
    shared: Tween,
    timer_progress: f32,
    timer_paused: bool,
}

impl Default for StatusIndicators {
    fn default() -> Self {
        Self {
            camera: Tween::new(0.0),
            microphone: Tween::new(0.0),
            timer: Tween::new(0.0),
            shared: Tween::new(0.0),
            timer_progress: 0.0,
            timer_paused: false,
        }
    }
}

impl StatusIndicators {
    pub(crate) fn update(
        &mut self,
        state: DeviceActivity,
        timer: Option<TimerContent>,
        now: Instant,
    ) -> bool {
        let before = self.activity();
        let camera = state.camera == Some(true);
        let microphone = state.microphone == Some(true);
        let timing = timer.is_some();
        let devices = camera || microphone;
        let pace = |other: bool| if other { SHARE_DURATION } else { FADE_DURATION };
        for (animation, active, duration) in [
            (&mut self.camera, camera, pace(timing)),
            (&mut self.microphone, microphone, pace(timing)),
            (&mut self.timer, timing, pace(devices)),
            (&mut self.shared, devices && timing, SHARE_DURATION),
        ] {
            animation.retarget(if active { 1.0 } else { 0.0 }, duration, Curve::Ease, now);
            animation.update(now);
        }
        if let Some(timer) = timer {
            self.timer_progress = (timer.progress() * 720.0).round() / 720.0;
            self.timer_paused = timer.paused;
        }
        before != self.activity()
    }

    pub(crate) fn activity(&self) -> Option<Activity> {
        let camera = self.camera.value();
        let microphone = self.microphone.value();
        let timer = self.timer.value();
        (camera > 0.0 || microphone > 0.0 || timer > 0.0 || self.is_animating()).then_some(
            Activity::Status {
                camera,
                microphone,
                timer,
                shared: self.shared.value(),
                timer_progress: self.timer_progress,
                timer_paused: self.timer_paused,
            },
        )
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.camera.is_animating()
            || self.microphone.is_animating()
            || self.timer.is_animating()
            || self.shared.is_animating()
    }
}

pub(super) struct Indicators {
    pub(super) camera: f32,
    pub(super) microphone: f32,
    pub(super) timer: f32,
    pub(super) timer_progress: f32,
    pub(super) timer_paused: bool,
}

pub(super) fn width(height: f32, shared: f32) -> f32 {
    height * (1.0 + PILL_EXTENSION * shared)
}

pub(super) fn draw(
    painter: Painter<'_>,
    rect: Rect,
    _scale: f32,
    opacity: f32,
    indicators: Indicators,
) {
    let diameter = rect.width().min(rect.height());
    if indicators.timer > 0.0 {
        crate::ui::compact::timer::draw_timer_icon(
            painter,
            Point::new(rect.left + diameter / 2.0, rect.center_y()),
            diameter / TIMER_ICON_DIAMETER,
            indicators.timer_progress,
            indicators.timer_paused,
            opacity * indicators.timer * indicators.timer,
        );
    }
    let dot_radius = diameter / 9.0;
    let dot_spacing = diameter * 0.35;
    let dots_x = rect.right - diameter / 2.0;
    let dots = [
        (
            indicators.camera,
            -indicators.microphone,
            Rgba::from_rgb(48, 209, 88),
        ),
        (
            indicators.microphone,
            indicators.camera,
            Rgba::from_rgb(255, 159, 10),
        ),
    ];
    for (visibility, offset, color) in dots {
        let x = dots_x + offset * 0.5 * dot_spacing;
        painter.fill_circle(
            Point::new(x, rect.center_y()),
            dot_radius,
            color.with_alpha_f(opacity * visibility * visibility),
        );
    }
}
