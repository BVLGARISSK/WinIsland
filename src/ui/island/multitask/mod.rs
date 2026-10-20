pub(crate) mod activities;
mod geometry;

use std::collections::HashMap;

pub(crate) use geometry::{IslandSurface, surface};

use winisland_core::multitask::TaskFrame;
use winisland_plugin_host::draw::replay::PreparedFrame;
use winisland_plugin_host::host::PluginHost;
use winisland_render::{Painter, Rect};

use activities::Activity;

pub(crate) fn side_extent(scale: f32, compact_height: f32) -> f32 {
    compact_height * 1.1 + 20.0 * scale
}

pub(crate) fn draw_content(
    painter: Painter<'_>,
    surface: &IslandSurface,
    frame: Option<TaskFrame<Activity>>,
    scale: f32,
    plugins: Option<(&PluginHost, &HashMap<u64, PreparedFrame>)>,
) {
    if let (Some(rect), Some(frame)) = (surface.secondary, frame) {
        painter.save();
        painter.clip_path(&surface.outline);
        frame
            .task
            .content
            .draw(painter, rect, scale, frame.content_opacity);
        if let (Some((host, frames)), Some((id, visibility))) =
            (plugins, frame.task.content.plugin_surface())
        {
            let diameter = rect.width().min(rect.height());
            let alpha = frame.content_opacity * visibility * visibility;
            crate::ui::plugin::draw(
                painter,
                host,
                frames,
                crate::ui::plugin::SurfaceFrame {
                    id,
                    rect: Rect::from_xywh(
                        rect.left,
                        rect.center_y() - diameter / 2.0,
                        diameter,
                        diameter,
                    ),
                    logical: [diameter / scale; 2],
                    input_offset: 0.0,
                    alpha: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                    interactive: false,
                },
            );
        }
        painter.restore();
    }
}
