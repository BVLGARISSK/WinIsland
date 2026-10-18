mod device;

pub(crate) use device::StatusIndicators;

use winisland_render::{Painter, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Activity {
    Status {
        camera: f32,
        microphone: f32,
        timer: f32,
        shared: f32,
        timer_progress: f32,
        timer_paused: bool,
    },
}

impl Activity {
    pub(crate) fn width(self, height: f32, _scale: f32) -> f32 {
        match self {
            Self::Status { shared, .. } => device::width(height, shared),
        }
    }

    pub(crate) fn has_timer(self) -> bool {
        match self {
            Self::Status { timer, .. } => timer > 0.5,
        }
    }

    pub(crate) fn draw(self, painter: Painter<'_>, rect: Rect, scale: f32, opacity: f32) {
        match self {
            Self::Status {
                camera,
                microphone,
                timer,
                timer_progress,
                timer_paused,
                ..
            } => device::draw(
                painter,
                rect,
                scale,
                opacity,
                device::Indicators {
                    camera,
                    microphone,
                    timer,
                    timer_progress,
                    timer_paused,
                },
            ),
        }
    }
}
