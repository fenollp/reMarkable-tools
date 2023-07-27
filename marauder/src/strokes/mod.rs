use std::{thread::sleep, time::Duration};

use itertools::Itertools;
use libremarkable::{
    appctx::ApplicationContext,
    framebuffer::{
        cgmath, cgmath::EuclideanSpace, common::*, FramebufferDraw, FramebufferRefresh,
        PartialRefreshMode,
    },
};

use crate::{modes::draw::*, unipen};

pub struct Stroke {
    color: color,
    tip_size: u32,
    step: Duration,
    points_and_pressure: Vec<(cgmath::Point2<f32>, u16)>,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke {
            color: color::default(),
            tip_size: DrawMode::default().get_size(),
            step: Duration::from_millis(0),
            points_and_pressure: Vec::new(),
        }
    }
}

impl Stroke {
    pub fn set_color(&mut self, x: color) {
        self.color = x;
    }
    pub fn set_tip_size(&mut self, x: u32) {
        self.tip_size = x;
    }
    pub fn set_step(&mut self, x: Duration) {
        self.step = x;
    }

    pub fn set_points_and_pressure(&mut self, x: &[(cgmath::Point2<f32>, u16)]) {
        self.points_and_pressure = x.to_owned();
    }

    pub fn invert_color(&mut self) {
        let c = match self.color {
            color::BLACK => color::WHITE,
            _ => color::BLACK,
        };
        self.color = c;
    }

    pub fn push_back(&mut self, p: cgmath::Point2<f32>, pressure: u16) {
        self.points_and_pressure.push((p, pressure));
    }

    pub fn clear(&mut self) {
        self.points_and_pressure.clear();
    }

    pub fn translate(&mut self, (dx, dy): (f32, f32)) {
        for (p, _) in &mut self.points_and_pressure {
            p.x += dx;
            p.y += dy;
        }
    }

    pub fn pointwidth(&self, pressure: u16) -> f32 {
        (self.tip_size as f32) * (f32::from(pressure)) / 2048.
    }

    pub fn draw(&self, app: &mut ApplicationContext) {
        for (start, ctrl, end) in self.points_and_pressure.iter().tuple_windows() {
            let points = vec![start, ctrl, end];
            let radii: Vec<f32> =
                points.iter().map(|(_, pressure)| (self.pointwidth(*pressure) / 2.)).collect();
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
                self.color,
            );
            framebuffer.partial_refresh(
                &rect,
                PartialRefreshMode::Async,
                waveform_mode::WAVEFORM_MODE_DU,
                display_temp::TEMP_USE_REMARKABLE_DRAW,
                dither_mode::EPDC_FLAG_EXP1,
                DRAWING_QUANT_BIT,
                false,
            );
            if self.step > Duration::from_millis(1) {
                sleep(self.step)
            }
        }
    }
}

// TODO: change these (store?)
fn canvas_width() -> f32 {
    1404.
}
fn canvas_height() -> f32 {
    1080.
}

pub struct Strokes {
    strokes: Vec<Stroke>,
}

impl From<Vec<Stroke>> for Strokes {
    fn from(strokes: Vec<Stroke>) -> Self {
        Strokes { strokes }
    }
}

impl Strokes {
    pub fn draw(&self, app: &mut ApplicationContext) {
        for stroke in self.strokes.iter() {
            stroke.draw(app);
            sleep(2 * stroke.step);
        }
    }

    fn bounding_box(&self) -> (f32, f32, f32, f32) {
        let (mut xmin, mut xmax) = (f32::MAX, f32::MIN);
        let (mut ymin, mut ymax) = (f32::MAX, f32::MIN);
        for stroke in self.strokes.iter() {
            for (xy, p) in stroke.points_and_pressure.iter() {
                let radi = stroke.pointwidth(*p) / 2.;
                let (x, y) = (abs_add(xy.x, radi), abs_add(xy.y, radi));
                xmin = if x < xmin { x } else { xmin };
                xmax = if x > xmax { x } else { xmax };
                ymin = if y < ymin { y } else { ymin };
                ymax = if y > ymax { y } else { ymax };
                // let mut min_y = pos.y.floor().max(0.0) as u32;
                // let mut max_y = pos.y.ceil().max(0.0) as u32;
                // let mut min_x = pos.x.floor().max(0.0) as u32;
                // let mut max_x = pos.x.ceil().max(0.0) as u32;
            }
        }
        (xmin, xmax, ymin, ymax)
    }

    pub fn translation_boundaries(&self) -> (f32, f32, f32, f32) {
        let (xmin, xmax, ymin, ymax) = self.bounding_box();
        let left = (canvas_width()) - xmin;
        let right = (canvas_width()) - xmax;
        let top = (canvas_height()) - ymin;
        let bottom = (canvas_height()) - ymax;
        let width = xmax - xmin;
        let height = ymax - ymin;
        (
            // xs
            -(left - width),
            right - width,
            // ys
            -(top - height),
            bottom - height,
        )
    }

    pub fn translate(&mut self, (dx, dy): (f32, f32)) {
        for stroke in &mut self.strokes {
            stroke.translate((dx, dy));
        }
    }

    pub fn from_ujipenchars(uji: &unipen::Word) -> Self {
        let mut strokes = vec![];
        for uji_stroke in &uji.strokes {
            let mut pnp: Vec<(cgmath::Point2<f32>, u16)> = Vec::new();
            pnp.reserve_exact(uji_stroke.len());
            for p in uji_stroke {
                pnp.push((
                    cgmath::Point2::<f32> { x: f32::from(p.x), y: f32::from(p.y) },
                    1024, // pressure
                ));
            }
            let mut stroke = Stroke::default();
            stroke.set_points_and_pressure(&pnp);
            stroke.set_color(color::BLACK);
            stroke.set_step(Duration::from_millis(2));
            strokes.push(stroke);
        }
        strokes.into()
    }
}

fn abs_add(p: f32, q: f32) -> f32 {
    let sign = if p.is_sign_negative() { -1. } else { 1. };
    sign * (p.abs() + q)
}

// struct NormPoint2 {
//     pos_x: f32,
//     pos_y: f32,
//     pressure: f32,
//     tilt_x: f32,
//     tilt_y: f32,
// }
// fn from_draw_event(
//     position: cgmath::Point2<f32>,
//     pressure: u16,
//     tilt: cgmath::Vector2<u16>,
// ) -> NormPoint2 {
//     let x = (position.x - (CANVAS_REGION.left as f32)) / (CANVAS_REGION.width as f32);
//     let y = (position.y - (CANVAS_REGION.top as f32)) / (CANVAS_REGION.height as f32);
//     let x = if x < 0.0 { 0.0 } else { x };
//     let x = if x > 1.0 { 1.0 } else { x };
//     let y = if y < 0.0 { 0.0 } else { y };
//     let y = if y > 1.0 { 1.0 } else { y };
//     NormPoint2 {
//         pos_x: x,
//         pos_y: y,
//         //
//         pressure: (pressure as f32) / (u16::MAX as f32),
//         // tilt
//         tilt_x: (tilt.x as f32) / (u16::MAX as f32),
//         tilt_y: (tilt.y as f32) / (u16::MAX as f32),
//     }
// }
// impl fmt::Display for NormPoint2 {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(
//             f,
//             "({}, {}) @{} /({},{})",
//             self.pos_x,
//             self.pos_y,
//             //
//             self.pressure,
//             //
//             self.tilt_x,
//             self.tilt_y,
//         )
//     }
// }
