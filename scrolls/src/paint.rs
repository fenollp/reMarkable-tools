use std::time::Duration;

use libremarkable::{
    appctx::ApplicationContext,
    framebuffer::{
        cgmath,
        cgmath::EuclideanSpace,
        common::{color, display_temp, dither_mode, waveform_mode, DRAWING_QUANT_BIT_3},
        FramebufferDraw, FramebufferRefresh, PartialRefreshMode,
    },
};
use pb::proto::hypercards::{drawing, Drawing};
use tokio::time::sleep;

pub(crate) const DRAWING_PACE: Duration = Duration::from_millis(2);
pub(crate) const INTER_DRAWING_PACE: Duration = Duration::from_millis(8);

pub(crate) async fn paint(app: &mut ApplicationContext<'_>, drawing: &Drawing) {
    if drawing.xs.len() < 3 {
        return;
    }

    let col = match drawing.color() {
        drawing::Color::White => color::WHITE,
        _ => color::BLACK,
    };
    for i in 0..(drawing.xs.len() - 2) {
        if i != 0 {
            sleep(DRAWING_PACE).await;
        }
        let points: Vec<(cgmath::Point2<f32>, i32, u32)> = vec![
            // start
            (
                cgmath::Point2 { x: drawing.xs[i], y: drawing.ys[i] },
                drawing.pressures[i],
                drawing.widths[i],
            ),
            // ctrl
            (
                cgmath::Point2 { x: drawing.xs[i + 1], y: drawing.ys[i + 1] },
                drawing.pressures[i + 1],
                drawing.widths[i + 1],
            ),
            // end
            (
                cgmath::Point2 { x: drawing.xs[i + 2], y: drawing.ys[i + 2] },
                drawing.pressures[i + 2],
                drawing.widths[i + 2],
            ),
        ];
        let radii: Vec<f32> = points
            .iter()
            .map(|(_, pressure, tip)| (((*tip as f32) * (*pressure as f32)) / 2048.) / 2.)
            .collect();
        // calculate control points
        let start_point = points[2].0.midpoint(points[1].0);
        let ctrl_point = points[1].0;
        let end_point = points[1].0.midpoint(points[0].0);
        // calculate diameters
        let start_width = radii[2] + radii[1];
        let ctrl_width = radii[1] * 2.;
        let end_width = radii[1] + radii[0];

        let framebuffer = app.get_framebuffer_ref();
        let rect = framebuffer.draw_dynamic_bezier(
            (start_point, start_width),
            (ctrl_point, ctrl_width),
            (end_point, end_width),
            10,
            col,
        );
        framebuffer.partial_refresh(
            &rect,
            PartialRefreshMode::Async,
            waveform_mode::WAVEFORM_MODE_DU,
            display_temp::TEMP_USE_REMARKABLE_DRAW,
            dither_mode::EPDC_FLAG_EXP1,
            DRAWING_QUANT_BIT_3,
            false,
        );
    }
}
