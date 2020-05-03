#![feature(nll)]
#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate libremarkable;
use libremarkable::framebuffer::cgmath;
use libremarkable::framebuffer::cgmath::EuclideanSpace;
use libremarkable::framebuffer::common::*;
use libremarkable::framebuffer::refresh::PartialRefreshMode;
use libremarkable::framebuffer::storage;
use libremarkable::framebuffer::{FramebufferDraw, FramebufferIO, FramebufferRefresh};
use libremarkable::image::GenericImage;
use libremarkable::input::{gpio, multitouch, wacom, InputDevice};
use libremarkable::ui_extensions::element::{
    UIConstraintRefresh, UIElement, UIElementHandle, UIElementWrapper,
};
use libremarkable::{appctx, battery, image};

extern crate chrono;
use chrono::{DateTime, Local};

extern crate atomic;
use atomic::Atomic;

extern crate rand;
use rand::Rng;

use itertools::Itertools;

use std::collections::VecDeque;
use std::fmt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

#[derive(Copy, Clone, PartialEq)]
enum DrawMode {
    Draw(u32),
    Erase(u32),
}
impl DrawMode {
    fn default() -> Self {
        DrawMode::Draw(2)
    }
    fn set_size(self, new_size: u32) -> Self {
        match self {
            DrawMode::Draw(_) => DrawMode::Draw(new_size),
            DrawMode::Erase(_) => DrawMode::Erase(new_size),
        }
    }
    fn color_as_string(self) -> String {
        match self {
            DrawMode::Draw(_) => "Black",
            DrawMode::Erase(_) => "White",
        }
        .into()
    }
    fn get_size(self) -> u32 {
        match self {
            DrawMode::Draw(s) => s,
            DrawMode::Erase(s) => s,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum TouchMode {
    OnlyUI,
    Bezier,
    Circles,
    Diamonds,
    FillDiamonds,
}
impl TouchMode {
    fn toggle(self) -> Self {
        match self {
            TouchMode::OnlyUI => TouchMode::Bezier,
            TouchMode::Bezier => TouchMode::Circles,
            TouchMode::Circles => TouchMode::Diamonds,
            TouchMode::Diamonds => TouchMode::FillDiamonds,
            TouchMode::FillDiamonds => TouchMode::OnlyUI,
        }
    }
}
impl fmt::Display for TouchMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TouchMode::OnlyUI => write!(f, "None"),
            TouchMode::Bezier => write!(f, "Bezier"),
            TouchMode::Circles => write!(f, "Circles"),
            TouchMode::Diamonds => write!(f, "Diamonds"),
            TouchMode::FillDiamonds => write!(f, "FDiamonds"),
        }
    }
}

struct Strokes {
    strokes: Vec<Stroke>,
}
impl From<Vec<Stroke>> for Strokes {
    fn from(strokes: Vec<Stroke>) -> Self {
        Strokes { strokes }
    }
}
impl Strokes {
    fn draw(&self, app: &mut appctx::ApplicationContext) {
        for stroke in self.strokes.iter() {
            stroke.draw(app);
            sleep(2 * stroke.step);
        }
    }
    fn approximate_rect(&self) -> (f32, f32, f32, f32) {
        let (mut xmin, mut xmax) = (CANVAS_REGION.width as f32, 0.);
        let (mut ymin, mut ymax) = (CANVAS_REGION.height as f32, 0.);
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
    fn translation_boundaries(&self) -> (f32, f32, f32, f32) {
        let (xmin, xmax, ymin, ymax) = self.approximate_rect();
        let left = (CANVAS_REGION.width as f32) - xmin;
        let right = (CANVAS_REGION.width as f32) - xmax;
        let top = (CANVAS_REGION.height as f32) - ymin;
        let bottom = (CANVAS_REGION.height as f32) - ymax;
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
    fn translate(&mut self, (dx, dy): (f32, f32)) {
        for stroke in &mut self.strokes {
            stroke.translate((dx, dy));
        }
    }
}

struct Stroke {
    color: color,
    tip_size: u32,
    step: Duration,
    points_and_pressure: Vec<(cgmath::Point2<f32>, u16)>,
}
impl Stroke {
    fn new() -> Self {
        Stroke {
            color: color::default(),
            tip_size: DrawMode::default().get_size(),
            step: Duration::from_millis(0),
            points_and_pressure: Vec::new(),
        }
    }
    fn set_color(&mut self, color: color) {
        self.color = color;
    }
    fn set_tip_size(&mut self, tip_size: u32) {
        self.tip_size = tip_size;
    }
    fn set_step(&mut self, x: Duration) {
        self.step = x;
    }
    fn set_points_and_pressure(&mut self, x: &[(cgmath::Point2<f32>, u16)]) {
        self.points_and_pressure = x.to_owned();
    }
    fn invert_color(&mut self) {
        match self.color {
            color::WHITE => self.color = color::BLACK,
            _ => self.color = color::BLACK,
        }
    }
    fn push_back(&mut self, p: cgmath::Point2<f32>, pressure: u16) {
        self.points_and_pressure.push((p, pressure));
    }
    fn clear(&mut self) {
        self.points_and_pressure.clear();
    }
    fn translate(&mut self, (dx, dy): (f32, f32)) {
        for (p, _) in &mut self.points_and_pressure {
            p.x += dx;
            p.y += dy;
        }
    }
    fn draw(&self, app: &mut appctx::ApplicationContext) {
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

// This region will have the following size at rest:
//   raw: 5896 kB
//   zstd: 10 kB
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 720,
    left: 0,
    height: 1080,
    width: 1404,
};

lazy_static! {
    static ref G_TOUCH_MODE: Atomic<TouchMode> = Atomic::new(TouchMode::OnlyUI);
    static ref G_DRAW_MODE: Atomic<DrawMode> = Atomic::new(DrawMode::default());
    static ref UNPRESS_OBSERVED: AtomicBool = AtomicBool::new(false);
    static ref WACOM_IN_RANGE: AtomicBool = AtomicBool::new(false);
    static ref WACOM_HISTORY: Mutex<VecDeque<(cgmath::Point2<f32>, i32)>> =
        Mutex::new(VecDeque::new());
    static ref WACOM_UNDO: Mutex<Stroke> = Mutex::new(Stroke::new());
    static ref WACOM_UNDO_TICK: AtomicBool = AtomicBool::new(true);
    static ref G_COUNTER: Mutex<u32> = Mutex::new(0);
    static ref LAST_REFRESHED_CANVAS_RECT: Atomic<mxcfb_rect> = Atomic::new(mxcfb_rect::invalid());
    static ref SAVED_CANVAS: Mutex<Option<storage::CompressedCanvasState>> = Mutex::new(None);
}

// ####################
// ## Button Handlers
// ####################

fn on_undo(app: &mut appctx::ApplicationContext, _element: UIElementHandle) {
    let mut wacom_undo = WACOM_UNDO.lock().unwrap();
    wacom_undo.invert_color();
    wacom_undo.draw(app);
    wacom_undo.clear();
}

fn on_save_canvas(app: &mut appctx::ApplicationContext, _element: UIElementHandle) {
    let framebuffer = app.get_framebuffer_ref();
    match framebuffer.dump_region(CANVAS_REGION) {
        Err(err) => error!("Failed to dump buffer: {0}", err),
        Ok(buff) => {
            let mut hist = SAVED_CANVAS.lock().unwrap();
            *hist = Some(storage::CompressedCanvasState::new(
                buff.as_slice(),
                CANVAS_REGION.height,
                CANVAS_REGION.width,
            ));
        }
    };
}

fn on_zoom_out(app: &mut appctx::ApplicationContext, _element: UIElementHandle) {
    let framebuffer = app.get_framebuffer_ref();
    match framebuffer.dump_region(CANVAS_REGION) {
        Err(err) => error!("Failed to dump buffer: {0}", err),
        Ok(buff) => {
            let resized = image::DynamicImage::ImageRgb8(
                storage::rgbimage_from_u8_slice(
                    CANVAS_REGION.width,
                    CANVAS_REGION.height,
                    buff.as_slice(),
                )
                .unwrap(),
            )
            .resize(
                (CANVAS_REGION.width as f32 / 1.25f32) as u32,
                (CANVAS_REGION.height as f32 / 1.25f32) as u32,
                image::imageops::Nearest,
            );

            // Get a clean image the size of the canvas
            let mut new_image =
                image::DynamicImage::new_rgb8(CANVAS_REGION.width, CANVAS_REGION.height);
            new_image.invert();

            // Copy the resized image into the subimage
            new_image.copy_from(&resized, CANVAS_REGION.width / 8, CANVAS_REGION.height / 8);

            framebuffer.draw_image(
                &new_image.as_rgb8().unwrap(),
                CANVAS_REGION.top_left().cast().unwrap(),
            );
            framebuffer.partial_refresh(
                &CANVAS_REGION,
                PartialRefreshMode::Async,
                waveform_mode::WAVEFORM_MODE_GC16_FAST,
                display_temp::TEMP_USE_REMARKABLE_DRAW,
                dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                0,
                false,
            );
        }
    };
}

fn on_blur_canvas(app: &mut appctx::ApplicationContext, _element: UIElementHandle) {
    let framebuffer = app.get_framebuffer_ref();
    match framebuffer.dump_region(CANVAS_REGION) {
        Err(err) => error!("Failed to dump buffer: {0}", err),
        Ok(buff) => {
            let dynamic = image::DynamicImage::ImageRgb8(
                storage::rgbimage_from_u8_slice(
                    CANVAS_REGION.width,
                    CANVAS_REGION.height,
                    buff.as_slice(),
                )
                .unwrap(),
            )
            .blur(0.6f32);

            framebuffer.draw_image(
                &dynamic.as_rgb8().unwrap(),
                CANVAS_REGION.top_left().cast().unwrap(),
            );
            framebuffer.partial_refresh(
                &CANVAS_REGION,
                PartialRefreshMode::Async,
                waveform_mode::WAVEFORM_MODE_GC16_FAST,
                display_temp::TEMP_USE_REMARKABLE_DRAW,
                dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                0,
                false,
            );
        }
    };
}

fn on_invert_canvas(app: &mut appctx::ApplicationContext, element: UIElementHandle) {
    let framebuffer = app.get_framebuffer_ref();
    match framebuffer.dump_region(CANVAS_REGION) {
        Err(err) => error!("Failed to dump buffer: {0}", err),
        Ok(mut buff) => {
            buff.iter_mut().for_each(|p| {
                *p = !(*p);
            });
            match framebuffer.restore_region(CANVAS_REGION, &buff) {
                Err(e) => error!("Error while restoring region: {0}", e),
                Ok(_) => {
                    framebuffer.partial_refresh(
                        &CANVAS_REGION,
                        PartialRefreshMode::Async,
                        waveform_mode::WAVEFORM_MODE_GC16_FAST,
                        display_temp::TEMP_USE_REMARKABLE_DRAW,
                        dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                        0,
                        false,
                    );
                }
            };
        }
    };

    // Invert the draw color as well for more natural UX
    on_toggle_eraser(app, element);
}

fn on_load_canvas(app: &mut appctx::ApplicationContext, _element: UIElementHandle) {
    match *SAVED_CANVAS.lock().unwrap() {
        None => {}
        Some(ref compressed_state) => {
            let framebuffer = app.get_framebuffer_ref();
            let decompressed = compressed_state.decompress();

            match framebuffer.restore_region(CANVAS_REGION, &decompressed) {
                Err(e) => error!("Error while restoring region: {0}", e),
                Ok(_) => {
                    framebuffer.partial_refresh(
                        &CANVAS_REGION,
                        PartialRefreshMode::Async,
                        waveform_mode::WAVEFORM_MODE_GC16_FAST,
                        display_temp::TEMP_USE_REMARKABLE_DRAW,
                        dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                        0,
                        false,
                    );
                }
            };
        }
    };
}

fn on_toggle_eraser(app: &mut appctx::ApplicationContext, _: UIElementHandle) {
    let (new_mode, name) = match G_DRAW_MODE.load(Ordering::Relaxed) {
        DrawMode::Erase(s) => (DrawMode::Draw(s), "Black".to_owned()),
        DrawMode::Draw(s) => (DrawMode::Erase(s), "White".to_owned()),
    };
    G_DRAW_MODE.store(new_mode, Ordering::Relaxed);

    let indicator = app.get_element_by_name("colorIndicator");
    if let UIElement::Text { ref mut text, .. } = indicator.unwrap().write().inner {
        *text = name;
    }
    app.draw_element("colorIndicator");
}

fn on_change_touchdraw_mode(app: &mut appctx::ApplicationContext, _: UIElementHandle) {
    let new_val = G_TOUCH_MODE.load(Ordering::Relaxed).toggle();
    G_TOUCH_MODE.store(new_val, Ordering::Relaxed);

    let indicator = app.get_element_by_name("touchModeIndicator");
    if let UIElement::Text { ref mut text, .. } = indicator.unwrap().write().inner {
        *text = new_val.to_string();
    }
    // Make sure you aren't trying to draw the element while you are holding a write lock.
    // It doesn't seem to cause a deadlock however it may cause higher lock contention.
    app.draw_element("touchModeIndicator");
}

// ####################
// ## Miscellaneous
// ####################

fn change_brush_width(app: &mut appctx::ApplicationContext, delta: i32) {
    let current = G_DRAW_MODE.load(Ordering::Relaxed);
    let current_size = current.get_size() as i32;
    let proposed_size = current_size + delta;
    let new_size = if proposed_size < 1 {
        1
    } else if proposed_size > 99 {
        99
    } else {
        proposed_size
    };
    if new_size == current_size {
        return;
    }

    G_DRAW_MODE.store(current.set_size(new_size as u32), Ordering::Relaxed);

    let element = app.get_element_by_name("displaySize").unwrap();
    if let UIElement::Text { ref mut text, .. } = element.write().inner {
        *text = format!("size: {0}", new_size);
    }
    app.draw_element("displaySize");
}

fn loop_update_topbar(app: &mut appctx::ApplicationContext, millis: u64) {
    let time_label = app.get_element_by_name("time").unwrap();
    let battery_label = app.get_element_by_name("battery").unwrap();
    loop {
        // Get the datetime
        let dt: DateTime<Local> = Local::now();

        if let UIElement::Text { ref mut text, .. } = time_label.write().inner {
            *text = format!("{}", dt.format("%F %r"));
        }

        if let UIElement::Text { ref mut text, .. } = battery_label.write().inner {
            *text = format!(
                "{0:<128}",
                format!(
                    "{0} — {1}%",
                    battery::human_readable_charging_status().unwrap(),
                    battery::percentage().unwrap()
                )
            );
        }
        app.draw_element("time");
        app.draw_element("battery");
        sleep(Duration::from_millis(millis));
    }
}

fn strokes_smileyface(c: color, step: Duration) -> Strokes {
    let mut left_eye = Stroke::new();
    left_eye.set_points_and_pressure(&[
        ((1167.1279, 780.51337).into(), 1978),
        ((1166.9495, 780.60266).into(), 2167),
        ((1166.8601, 780.69196).into(), 2458),
        ((1166.9495, 780.60266).into(), 2725),
        ((1166.9495, 780.51337).into(), 2842),
        ((1166.9495, 780.51337).into(), 2839),
        ((1167.0387, 780.51337).into(), 2765),
        ((1167.1279, 780.51337).into(), 2603),
        ((1167.2173, 780.60266).into(), 2424),
        ((1167.5745, 780.69196).into(), 2256),
        ((1168.3779, 780.78125).into(), 2029),
        ((1169.2709, 781.0491).into(), 1709),
        ((1170.1637, 781.22766).into(), 1490),
        ((1171.2351, 781.31696).into(), 1441),
        ((1171.9493, 782.0312).into(), 916),
        ((1173.0208, 782.1205).into(), 10),
    ]);
    // (0.8312877, 0.056030896) @0.030182345 /(0.99696344,0.05035477)
    // (0.8311606, 0.056113575) @0.0330663 /(0.99696344,0.05035477)
    // (0.83109695, 0.056196254) @0.037506677 /(0.99696344,0.05035477)
    // (0.8311606, 0.056113575) @0.041580833 /(0.99696344,0.05035477)
    // (0.8311606, 0.056030896) @0.04336614 /(0.99696344,0.05035477)
    // (0.8311606, 0.056030896) @0.04332036 /(0.99696344,0.05035477)
    // (0.83122414, 0.056030896) @0.042191196 /(0.99696344,0.05035477)
    // (0.8312877, 0.056030896) @0.039719235 /(0.99696344,0.05035477)
    // (0.83135134, 0.056113575) @0.03698787 /(0.99696344,0.05035477)
    // (0.83160573, 0.056196254) @0.034424353 /(0.99696344,0.05035477)
    // (0.832178, 0.056278937) @0.030960556 /(0.99696344,0.05035477)
    // (0.83281404, 0.056526918) @0.02607767 /(0.99696344,0.05035477)
    // (0.8334499, 0.05669228) @0.022735942 /(0.99696344,0.05035477)
    // (0.834213, 0.05677496) @0.02198825 /(0.99696344,0.05035477)
    // (0.83472174, 0.057436287) @0.013977264 /(0.99696344,0.05035477)
    // (0.83548486, 0.057518966) @0.00015259022 /(0.99696344,0.05035477)

    let mut right_eye = Stroke::new();
    right_eye.set_points_and_pressure(&[
        ((1183.6456, 784.174).into(), 1434),
        ((1183.8242, 784.0847).into(), 1502),
        ((1184.1814, 783.7276).into(), 1742),
        ((1184.1814, 783.549).into(), 2113),
        ((1184.1814, 783.4597).into(), 2402),
        ((1184.2706, 783.3705).into(), 2544),
        ((1184.3599, 783.2812).into(), 2609),
        ((1184.3599, 783.1919).into(), 2640),
        ((1184.4492, 783.0133).into(), 2650),
        ((1184.6278, 782.924).into(), 2638),
        ((1184.8956, 782.8348).into(), 2599),
        ((1185.0742, 782.6562).into(), 2532),
        ((1185.4313, 782.5669).into(), 2403),
        ((1185.8777, 782.2991).into(), 2113),
        ((1186.4135, 782.1205).into(), 1683),
        ((1185.7885, 783.0133).into(), 1229),
        ((1186.5027, 782.924).into(), 7),
    ]);
    // (0.84305245, 0.05942038) @0.021881437 /(0,0.044251163)
    // (0.84317964, 0.0593377) @0.022919051 /(0,0.044251163)
    // (0.84343404, 0.059007037) @0.026581217 /(0,0.044251163)
    // (0.84343404, 0.058841676) @0.032242313 /(0,0.044251163)
    // (0.84343404, 0.058758996) @0.03665217 /(0,0.044251163)
    // (0.8434976, 0.058676373) @0.03881895 /(0,0.044251163)
    // (0.8435612, 0.058593694) @0.039810788 /(0,0.044251163)
    // (0.8435612, 0.058511015) @0.040283818 /(0,0.044251163)
    // (0.8436248, 0.058345653) @0.04043641 /(0,0.044251163)
    // (0.843752, 0.058262974) @0.0402533 /(0,0.044251163)
    // (0.84394276, 0.05818035) @0.039658196 /(0,0.044251163)
    // (0.84406996, 0.05801499) @0.038635843 /(0,0.044251163)
    // (0.8443243, 0.05793231) @0.03666743 /(0,0.044251163)
    // (0.8446422, 0.057684325) @0.032242313 /(0,0.044251163)
    // (0.8450238, 0.057518966) @0.025680933 /(0,0.044251163)
    // (0.8445787, 0.058345653) @0.018753339 /(0,0.044251163)
    // (0.8450874, 0.058262974) @0.00010681315 /(0,0.044251163)

    let mut smile = Stroke::new();
    smile.set_points_and_pressure(&[
        ((1166.0565, 794.0844).into(), 1298),
        ((1166.1459, 793.9952).into(), 1350),
        ((1166.2351, 793.8166).into(), 1389),
        ((1166.2351, 793.7273).into(), 1406),
        ((1166.1459, 793.4595).into(), 1417),
        ((1166.1459, 793.3702).into(), 1430),
        ((1166.1459, 793.3702).into(), 1452),
        ((1166.0565, 793.3702).into(), 1497),
        ((1165.9673, 793.4595).into(), 1574),
        ((1165.9673, 793.4595).into(), 1690),
        ((1165.9673, 793.5487).into(), 1838),
        ((1165.878, 793.5487).into(), 1969),
        ((1165.7887, 793.7273).into(), 2036),
        ((1165.6995, 793.9952).into(), 2052),
        ((1165.5209, 794.1737).into(), 2056),
        ((1165.4315, 794.4416).into(), 2061),
        ((1165.3423, 794.7987).into(), 2061),
        ((1165.3423, 795.1558).into(), 2051),
        ((1165.253, 795.51294).into(), 2033),
        ((1165.1637, 795.95935).into(), 2016),
        ((1165.1637, 796.3165).into(), 2005),
        ((1165.253, 796.67365).into(), 1999),
        ((1165.253, 796.94147).into(), 1994),
        ((1165.4315, 797.20935).into(), 1996),
        ((1165.6101, 797.56647).into(), 1992),
        ((1165.6995, 797.74506).into(), 1982),
        ((1165.9673, 798.0129).into(), 1988),
        ((1166.2351, 798.37).into(), 2016),
        ((1166.6815, 798.81647).into(), 2052),
        ((1167.4851, 799.3521).into(), 2084),
        ((1168.1101, 799.7986).into(), 2109),
        ((1168.8243, 800.1557).into(), 2124),
        ((1169.4493, 800.5128).into(), 2130),
        ((1170.2529, 800.78064).into(), 2125),
        ((1171.1458, 800.9592).into(), 2114),
        ((1171.9493, 801.0485).into(), 2110),
        ((1172.9315, 801.1378).into(), 2112),
        ((1174.0029, 801.0485).into(), 2108),
        ((1175.1636, 800.78064).into(), 2096),
        ((1176.3243, 800.4235).into(), 2089),
        ((1177.485, 799.9771).into(), 2084),
        ((1178.6458, 799.4414).into(), 2099),
        ((1179.8064, 798.7272).into(), 2121),
        ((1180.8778, 797.9236).into(), 2138),
        ((1181.86, 797.20935).into(), 2149),
        ((1182.842, 796.4058).into(), 2163),
        ((1184.0028, 795.60223).into(), 2183),
        ((1184.717, 794.888).into(), 2204),
        ((1185.1635, 794.3523).into(), 2225),
        ((1185.6992, 793.8166).into(), 2224),
        ((1185.967, 793.3702).into(), 2221),
        ((1186.1456, 793.1023).into(), 2230),
        ((1186.3242, 793.01306).into(), 2243),
        ((1186.3242, 792.8345).into(), 2267),
        ((1186.3242, 792.6559).into(), 2309),
        ((1186.2349, 792.47736).into(), 2343),
        ((1186.2349, 792.2095).into(), 2358),
        ((1186.2349, 791.85236).into(), 2355),
        ((1186.1456, 791.49524).into(), 2321),
        ((1186.1456, 791.13806).into(), 2244),
        ((1186.1456, 790.95953).into(), 2102),
        ((1186.1456, 790.87024).into(), 1860),
        ((1186.0563, 790.87024).into(), 1588),
        ((1185.967, 790.95953).into(), 1444),
        ((1186.8599, 790.24524).into(), 258),
    ]);
    // (0.83052456, 0.068596676) @0.01980621 /(0.0030518044,0.053406578)
    // (0.8305882, 0.06851406) @0.02059968 /(0.0030518044,0.053406578)
    // (0.83065176, 0.06834869) @0.021194782 /(0.0030518044,0.053406578)
    // (0.83065176, 0.06826601) @0.021454185 /(0.0030518044,0.053406578)
    // (0.8305882, 0.06801803) @0.021622034 /(0.0030518044,0.053406578)
    // (0.8305882, 0.06793535) @0.021820402 /(0.0030518044,0.053406578)
    // (0.8305882, 0.06793535) @0.0221561 /(0.0030518044,0.053406578)
    // (0.83052456, 0.06793535) @0.022842756 /(0.0030518044,0.053406578)
    // (0.830461, 0.06801803) @0.0240177 /(0.0030518044,0.053406578)
    // (0.830461, 0.06801803) @0.025787747 /(0.0030518044,0.053406578)
    // (0.830461, 0.06810065) @0.028046083 /(0.0030518044,0.053406578)
    // (0.8303975, 0.06810065) @0.030045014 /(0.0030518044,0.053406578)
    // (0.8303338, 0.06826601) @0.03106737 /(0.0030518044,0.053406578)
    // (0.8302703, 0.06851406) @0.031311512 /(0.0030518044,0.053406578)
    // (0.8301431, 0.068679355) @0.03137255 /(0.0030518044,0.053406578)
    // (0.83007944, 0.0689274) @0.031448845 /(0.0030518044,0.053406578)
    // (0.8300159, 0.069258064) @0.031448845 /(0.0030518044,0.053406578)
    // (0.8300159, 0.06958873) @0.031296253 /(0.0030518044,0.053406578)
    // (0.8299523, 0.069919385) @0.031021591 /(0.0030518044,0.053406578)
    // (0.8298887, 0.07033273) @0.030762188 /(0.0030518044,0.053406578)
    // (0.8298887, 0.07066345) @0.03059434 /(0.0030518044,0.053406578)
    // (0.8299523, 0.07099412) @0.030502785 /(0.0030518044,0.053406578)
    // (0.8299523, 0.0712421) @0.03042649 /(0.0030518044,0.053406578)
    // (0.83007944, 0.07149014) @0.030457009 /(0.0030518044,0.053406578)
    // (0.83020663, 0.0718208) @0.030395972 /(0.0030518044,0.053406578)
    // (0.8302703, 0.07198616) @0.030243382 /(0.0030518044,0.053406578)
    // (0.830461, 0.07223415) @0.030334936 /(0.0030518044,0.053406578)
    // (0.83065176, 0.07256481) @0.030762188 /(0.0030518044,0.053406578)
    // (0.83096975, 0.07297821) @0.031311512 /(0.0030518044,0.053406578)
    // (0.8315421, 0.073474176) @0.0317998 /(0.0030518044,0.053406578)
    // (0.83198726, 0.07388758) @0.032181278 /(0.0030518044,0.053406578)
    // (0.832496, 0.07421824) @0.032410163 /(0.0030518044,0.053406578)
    // (0.8329411, 0.07454891) @0.032501716 /(0.0030518044,0.053406578)
    // (0.8335135, 0.074796885) @0.032425422 /(0.0030518044,0.053406578)
    // (0.8341494, 0.07496225) @0.03225757 /(0.0030518044,0.053406578)
    // (0.83472174, 0.07504493) @0.032196537 /(0.0030518044,0.053406578)
    // (0.8354213, 0.07512761) @0.032227054 /(0.0030518044,0.053406578)
    // (0.83618444, 0.07504493) @0.03216602 /(0.0030518044,0.053406578)
    // (0.8370111, 0.074796885) @0.03198291 /(0.0030518044,0.053406578)
    // (0.8378379, 0.07446623) @0.0318761 /(0.0030518044,0.053406578)
    // (0.83866453, 0.07405288) @0.0317998 /(0.0030518044,0.053406578)
    // (0.83949125, 0.073556855) @0.032028686 /(0.0030518044,0.05035477)
    // (0.84031796, 0.07289553) @0.032364387 /(0.0030518044,0.05035477)
    // (0.8410811, 0.07215147) @0.03262379 /(0.0030518044,0.05035477)
    // (0.8417806, 0.07149014) @0.032791637 /(0.0030518044,0.05035477)
    // (0.84248006, 0.07074613) @0.033005264 /(0.0030518044,0.05035477)
    // (0.84330684, 0.07000207) @0.033310443 /(0.0030518044,0.05035477)
    // (0.84381557, 0.06934074) @0.033630885 /(0.0030518044,0.05035477)
    // (0.8441335, 0.06884472) @0.033951323 /(0.0030518044,0.05035477)
    // (0.8445151, 0.06834869) @0.033936065 /(0.0030518044,0.05035477)
    // (0.8447059, 0.06793535) @0.03389029 /(0.0030518044,0.05035477)
    // (0.8448331, 0.06768731) @0.034027617 /(0.0030518044,0.05035477)
    // (0.8449603, 0.06760468) @0.034225985 /(0.0030518044,0.05035477)
    // (0.8449603, 0.067439325) @0.034592204 /(0.0030518044,0.05035477)
    // (0.8449603, 0.06727397) @0.03523308 /(0.0030518044,0.05035477)
    // (0.8448966, 0.06710866) @0.035751887 /(0.0030518044,0.05035477)
    // (0.8448966, 0.06686062) @0.035980772 /(0.0030518044,0.05035477)
    // (0.8448966, 0.06652996) @0.035934996 /(0.0030518044,0.05035477)
    // (0.8448331, 0.066199295) @0.03541619 /(0.0030518044,0.05035477)
    // (0.8448331, 0.06586858) @0.034241244 /(0.0030518044,0.04882887)
    // (0.8448331, 0.06570327) @0.032074463 /(0.0030518044,0.04882887)
    // (0.8448331, 0.065620594) @0.028381782 /(0.0030518044,0.04882887)
    // (0.8447694, 0.065620594) @0.024231328 /(0.0030518044,0.04882887)
    // (0.8447059, 0.06570327) @0.022034029 /(0.0030518044,0.04882887)
    // (0.8453418, 0.065041885) @0.0039368276 /(0.0030518044,0.04882887)

    left_eye.set_color(c);
    right_eye.set_color(c);
    smile.set_color(c);

    left_eye.set_step(step);
    right_eye.set_step(step);
    smile.set_step(step);

    vec![left_eye, right_eye, smile].into()
}

fn loop_companion(app: &mut appctx::ApplicationContext) {
    let mut strokes = strokes_smileyface(color::BLACK, Duration::from_millis(2));
    let (xmin, xmax, ymin, ymax) = strokes.translation_boundaries();
    let mut rng = rand::thread_rng();

    loop {
        // select (dx,dy) such that strokes shifted by (dx,dy) is still within CANVAS_REGION
        let dx = rng.gen_range(xmin, xmax);
        let dy = rng.gen_range(ymin, ymax);
        strokes.translate((dx, dy));
        strokes.draw(app);
        sleep(Duration::from_millis(1_000));
    }
}

struct NormPoint2 {
    pos_x: f32,
    pos_y: f32,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
}
impl NormPoint2 {
    fn from_draw_event(
        position: cgmath::Point2<f32>,
        pressure: u16,
        tilt: cgmath::Vector2<u16>,
    ) -> NormPoint2 {
        let x = (position.x - (CANVAS_REGION.left as f32)) / (CANVAS_REGION.width as f32);
        let y = (position.y - (CANVAS_REGION.top as f32)) / (CANVAS_REGION.height as f32);
        let x = if x < 0.0 { 0.0 } else { x };
        let x = if x > 1.0 { 1.0 } else { x };
        let y = if y < 0.0 { 0.0 } else { y };
        let y = if y > 1.0 { 1.0 } else { y };
        NormPoint2 {
            pos_x: x,
            pos_y: y,
            //
            pressure: (pressure as f32) / (u16::MAX as f32),
            // tilt
            tilt_x: (tilt.x as f32) / (u16::MAX as f32),
            tilt_y: (tilt.y as f32) / (u16::MAX as f32),
        }
    }
}
impl fmt::Display for NormPoint2 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({}, {}) @{} /({},{})",
            self.pos_x,
            self.pos_y,
            //
            self.pressure,
            //
            self.tilt_x,
            self.tilt_y,
        )
    }
}

// ####################
// ## Input Handlers
// ####################

fn on_wacom_input(app: &mut appctx::ApplicationContext, input: wacom::WacomEvent) {
    match input {
        wacom::WacomEvent::Draw {
            position,
            pressure,
            tilt,
        } => {
            debug!("{} {} {}", position.x, position.y, pressure);
            debug!(
                ">>> {}",
                NormPoint2::from_draw_event(position, pressure, tilt)
            );

            let mut wacom_stack = WACOM_HISTORY.lock().unwrap();

            // This is so that we can click the buttons outside the canvas region
            // normally meant to be touched with a finger using our stylus
            if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
                wacom_stack.clear();
                if UNPRESS_OBSERVED.fetch_and(false, Ordering::Relaxed) {
                    let region = app
                        .find_active_region(position.y.round() as u16, position.x.round() as u16);
                    if let Some(element) = region.map(|(region, _)| region.element.clone()) {
                        (region.unwrap().0.handler)(app, element);
                    }
                }
                return;
            }

            let (col, mult) = match G_DRAW_MODE.load(Ordering::Relaxed) {
                DrawMode::Draw(s) => (color::BLACK, s),
                DrawMode::Erase(s) => (color::WHITE, s * 3),
            };

            wacom_stack.push_back((position.cast().unwrap(), pressure as i32));
            {
                let mut wacom_undo = WACOM_UNDO.lock().unwrap();
                if WACOM_UNDO_TICK.load(Ordering::Relaxed) {
                    WACOM_UNDO_TICK.store(false, Ordering::Relaxed);
                    wacom_undo.clear();
                    wacom_undo.set_color(col);
                    wacom_undo.set_tip_size(mult);
                }
                wacom_undo.push_back(position.cast().unwrap(), pressure);
            }

            while wacom_stack.len() >= 3 {
                let framebuffer = app.get_framebuffer_ref();
                let points = vec![
                    wacom_stack.pop_front().unwrap(),
                    *wacom_stack.get(0).unwrap(),
                    *wacom_stack.get(1).unwrap(),
                ];
                let radii: Vec<f32> = points
                    .iter()
                    .map(|point| ((mult as f32 * (point.1 as f32) / 2048.) / 2.0))
                    .collect();
                // calculate control points
                let start_point = points[2].0.midpoint(points[1].0);
                let ctrl_point = points[1].0;
                let end_point = points[1].0.midpoint(points[0].0);
                // calculate diameters
                let start_width = radii[2] + radii[1];
                let ctrl_width = radii[1] * 2.0;
                let end_width = radii[1] + radii[0];
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
                    DRAWING_QUANT_BIT,
                    false,
                );
            }
        }
        wacom::WacomEvent::InstrumentChange { pen, state } => {
            match pen {
                // Whether the pen is in range
                wacom::WacomPen::ToolPen => {
                    WACOM_IN_RANGE.store(state, Ordering::Relaxed);
                }
                // Whether the pen is actually making contact
                wacom::WacomPen::Touch => {
                    // Stop drawing when instrument has left the vicinity of the screen
                    if !state {
                        let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                        wacom_stack.clear();
                    }
                }
                _ => unreachable!(),
            }
        }
        wacom::WacomEvent::Hover {
            position: _,
            distance,
            tilt: _,
        } => {
            // If the pen is hovering, don't record its coordinates as the origin of the next line
            if distance > 1 {
                let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                wacom_stack.clear();
                UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
                WACOM_UNDO_TICK.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    };
}

fn on_touch_handler(app: &mut appctx::ApplicationContext, input: multitouch::MultitouchEvent) {
    let framebuffer = app.get_framebuffer_ref();
    if let multitouch::MultitouchEvent::Touch {
        gesture_seq: _,
        finger_id: _,
        position,
    } = input
    {
        if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
            return;
        }
        let rect = match G_TOUCH_MODE.load(Ordering::Relaxed) {
            TouchMode::Bezier => {
                let position_float = position.cast().unwrap();
                let points = vec![
                    (cgmath::vec2(-40.0, 0.0), 2.5),
                    (cgmath::vec2(40.0, -60.0), 5.5),
                    (cgmath::vec2(0.0, 0.0), 3.5),
                    (cgmath::vec2(-40.0, 60.0), 6.5),
                    (cgmath::vec2(-10.0, 50.0), 5.0),
                    (cgmath::vec2(10.0, 45.0), 4.5),
                    (cgmath::vec2(30.0, 55.0), 3.5),
                    (cgmath::vec2(50.0, 65.0), 3.0),
                    (cgmath::vec2(70.0, 40.0), 0.0),
                ];
                let mut rect = mxcfb_rect::invalid();
                for window in points.windows(3).step_by(2) {
                    rect = rect.merge_rect(&framebuffer.draw_dynamic_bezier(
                        (position_float + window[0].0, window[0].1),
                        (position_float + window[1].0, window[1].1),
                        (position_float + window[2].0, window[2].1),
                        100,
                        color::BLACK,
                    ));
                }
                rect
            }
            TouchMode::Circles => {
                framebuffer.draw_circle(position.cast().unwrap(), 20, color::BLACK)
            }

            m @ TouchMode::Diamonds | m @ TouchMode::FillDiamonds => {
                let position_int = position.cast().unwrap();
                framebuffer.draw_polygon(
                    &[
                        position_int + cgmath::vec2(-10, 0),
                        position_int + cgmath::vec2(0, 20),
                        position_int + cgmath::vec2(10, 0),
                        position_int + cgmath::vec2(0, -20),
                    ],
                    match m {
                        TouchMode::Diamonds => false,
                        TouchMode::FillDiamonds => true,
                        _ => false,
                    },
                    color::BLACK,
                )
            }
            _ => return,
        };
        framebuffer.partial_refresh(
            &rect,
            PartialRefreshMode::Async,
            waveform_mode::WAVEFORM_MODE_DU,
            display_temp::TEMP_USE_REMARKABLE_DRAW,
            dither_mode::EPDC_FLAG_USE_DITHERING_ALPHA,
            DRAWING_QUANT_BIT,
            false,
        );
    }
}

fn on_button_press(app: &mut appctx::ApplicationContext, input: gpio::GPIOEvent) {
    let (btn, new_state) = match input {
        gpio::GPIOEvent::Press { button } => (button, true),
        gpio::GPIOEvent::Unpress { button } => (button, false),
        _ => return,
    };

    // Ignoring the unpressed event
    if !new_state {
        return;
    }

    // Simple but effective accidental button press filtering
    if WACOM_IN_RANGE.load(Ordering::Relaxed) {
        return;
    }

    match btn {
        gpio::PhysicalButton::RIGHT => {
            let new_state = if app.is_input_device_active(InputDevice::Multitouch) {
                app.deactivate_input_device(InputDevice::Multitouch);
                "Enable Touch"
            } else {
                app.activate_input_device(InputDevice::Multitouch);
                "Disable Touch"
            };

            if let Some(ref elem) = app.get_element_by_name("tooltipRight") {
                if let UIElement::Text {
                    ref mut text,
                    scale: _,
                    foreground: _,
                    border_px: _,
                } = elem.write().inner
                {
                    *text = new_state.to_string();
                }
            }
            app.draw_element("tooltipRight");
        }
        gpio::PhysicalButton::MIDDLE | gpio::PhysicalButton::LEFT => {
            app.clear(btn == gpio::PhysicalButton::MIDDLE);
            app.draw_elements();
        }
        gpio::PhysicalButton::POWER => {
            Command::new("systemctl")
                .arg("start")
                .arg("xochitl")
                .spawn()
                .unwrap();
            std::process::exit(0);
        }
        gpio::PhysicalButton::WAKEUP => {
            info!("WAKEUP button(?) pressed(?)");
        }
    };
}

fn main() {
    env_logger::init();

    // Takes callback functions as arguments
    // They are called with the event and the &mut framebuffer
    let mut app: appctx::ApplicationContext =
        appctx::ApplicationContext::new(on_button_press, on_wacom_input, on_touch_handler);

    // Alternatively we could have called `app.execute_lua("fb.clear()")`
    app.clear(true);

    // Draw the borders for the canvas region
    app.add_element(
        "canvasRegion",
        UIElementWrapper {
            position: CANVAS_REGION.top_left().cast().unwrap() + cgmath::vec2(0, -2),
            refresh: UIConstraintRefresh::RefreshAndWait,
            onclick: None,
            inner: UIElement::Region {
                size: CANVAS_REGION.size().cast().unwrap() + cgmath::vec2(1, 3),
                border_px: 2,
                border_color: color::BLACK,
            },
            ..Default::default()
        },
    );

    // Zoom Out Button
    app.add_element(
        "zoomoutButton",
        UIElementWrapper {
            position: (960, 370).into(),
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_zoom_out),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Zoom Out".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    // Blur Toggle
    app.add_element(
        "blurToggle",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1155, y: 370 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_blur_canvas),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Blur".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    // Invert Toggle
    app.add_element(
        "invertToggle",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1247, y: 370 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_invert_canvas),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Invert".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );

    // Save/Restore Controls
    app.add_element(
        "saveButton",
        UIElementWrapper {
            position: cgmath::Point2 { x: 960, y: 440 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_save_canvas),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Save".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "restoreButton",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1080, y: 440 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_load_canvas),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Load".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "undoButton",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1200, y: 440 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_undo),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Undo".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );

    // Touch Mode Toggle
    app.add_element(
        "touchMode",
        UIElementWrapper {
            position: cgmath::Point2 { x: 960, y: 510 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_change_touchdraw_mode),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Touch Mode".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "touchModeIndicator",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1210, y: 510 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: None,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "None".to_owned(),
                scale: 40.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );

    // Color Mode Toggle
    app.add_element(
        "colorToggle",
        UIElementWrapper {
            position: cgmath::Point2 { x: 960, y: 580 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(on_toggle_eraser),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Draw Color".to_owned(),
                scale: 45.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "colorIndicator",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1210, y: 580 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: None,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: G_DRAW_MODE.load(Ordering::Relaxed).color_as_string(),
                scale: 40.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );

    // Size Controls
    app.add_element(
        "decreaseSizeSkip",
        UIElementWrapper {
            position: cgmath::Point2 { x: 960, y: 670 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, -10);
            }),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "--".to_owned(),
                scale: 90.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "decreaseSize",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1030, y: 670 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, -1);
            }),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "-".to_owned(),
                scale: 90.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "displaySize",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1080, y: 670 },
            refresh: UIConstraintRefresh::Refresh,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: format!("size: {0}", G_DRAW_MODE.load(Ordering::Relaxed).get_size()),
                scale: 45.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "increaseSize",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1240, y: 670 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, 1);
            }),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "+".to_owned(),
                scale: 60.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "increaseSizeSkip",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1295, y: 670 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, 10);
            }),
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "++".to_owned(),
                scale: 60.0,
                border_px: 5,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "exitToXochitl",
        UIElementWrapper {
            position: cgmath::Point2 { x: 30, y: 50 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: None,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Press POWER to return to reMarkable".to_owned(),
                scale: 35.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "tooltipLeft",
        UIElementWrapper {
            position: cgmath::Point2 { x: 15, y: 1850 },
            refresh: UIConstraintRefresh::Refresh,
            onclick: None,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Quick Redraw".to_owned(), // maybe quick redraw for the demo or waveform change?
                scale: 50.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "tooltipMiddle",
        UIElementWrapper {
            position: cgmath::Point2 { x: 565, y: 1850 },
            refresh: UIConstraintRefresh::Refresh,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Full Redraw".to_owned(),
                scale: 50.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "tooltipRight",
        UIElementWrapper {
            position: cgmath::Point2 { x: 1112, y: 1850 },
            refresh: UIConstraintRefresh::Refresh,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: "Disable Touch".to_owned(),
                scale: 50.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );

    // Create the top bar's time and battery labels. We can mutate these later.
    let dt: DateTime<Local> = Local::now();
    app.add_element(
        "battery",
        UIElementWrapper {
            position: cgmath::Point2 { x: 30, y: 215 },
            refresh: UIConstraintRefresh::Refresh,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: format!(
                    "{0:<128}",
                    format!(
                        "{0} — {1}%",
                        battery::human_readable_charging_status().unwrap(),
                        battery::percentage().unwrap()
                    )
                ),
                scale: 44.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "time",
        UIElementWrapper {
            position: cgmath::Point2 { x: 30, y: 150 },
            refresh: UIConstraintRefresh::Refresh,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: format!("{}", dt.format("%F %r")),
                scale: 75.0,
                border_px: 0,
            },
            ..Default::default()
        },
    );

    // Draw the scene
    app.draw_elements();

    // Get a &mut to the framebuffer object, exposing many convenience functions
    let appref = app.upgrade_ref();
    let clock_thread = std::thread::spawn(move || {
        loop_update_topbar(appref, 30 * 1000);
    });

    let appref2 = app.upgrade_ref();
    let companion_thread = std::thread::spawn(move || {
        loop_companion(appref2);
    });

    info!("Init complete. Beginning event dispatch...");

    // Blocking call to process events from digitizer + touchscreen + physical buttons
    app.dispatch_events(true, true, true);
    clock_thread.join().unwrap();
    companion_thread.join().unwrap();
}
