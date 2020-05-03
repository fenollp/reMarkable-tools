extern crate libremarkable;
use libremarkable::appctx;
use libremarkable::framebuffer::cgmath;
use libremarkable::framebuffer::cgmath::EuclideanSpace;
use libremarkable::framebuffer::common::*;
use libremarkable::framebuffer::refresh::PartialRefreshMode;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferRefresh};

use itertools::Itertools;

use std::thread::sleep;
use std::time::Duration;

use crate::modes::draw::*;

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
        match self.color {
            color::WHITE => self.color = color::BLACK,
            _ => self.color = color::BLACK,
        }
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

    pub fn draw(&self, app: &mut appctx::ApplicationContext) {
        let mult = self.tip_size as f32;
        for (start, ctrl, end) in self.points_and_pressure.iter().tuple_windows() {
            let points = vec![start, ctrl, end];
            let radii: Vec<f32> = points
                .iter()
                .map(|pandp| ((mult * (pandp.1 as f32) / 2048.) / 2.))
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
    pub fn draw(&self, app: &mut appctx::ApplicationContext) {
        for stroke in self.strokes.iter() {
            stroke.draw(app);
            sleep(2 * stroke.step);
        }
    }

    fn approximate_rect(&self) -> (f32, f32, f32, f32) {
        let (mut xmin, mut xmax) = (canvas_width(), 0.);
        let (mut ymin, mut ymax) = (canvas_height(), 0.);
        for stroke in self.strokes.iter() {
            for (p, _) in stroke.points_and_pressure.iter() {
                xmin = if p.x < xmin { p.x } else { xmin };
                xmax = if p.x > xmax { p.x } else { xmax };
                ymin = if p.y < ymin { p.y } else { ymin };
                ymax = if p.y > ymax { p.y } else { ymax };
            }
        }
        (xmin, xmax, ymin, ymax)
    }

    pub fn translation_boundaries(&self) -> (f32, f32, f32, f32) {
        let (xmin, xmax, ymin, ymax) = self.approximate_rect();
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
}
