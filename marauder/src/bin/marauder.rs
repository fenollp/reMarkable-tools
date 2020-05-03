#![feature(nll)]
#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate libremarkable;
use libremarkable::appctx::ApplicationContext;
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
use libremarkable::{battery, image};

extern crate chrono;
use chrono::{DateTime, Local};

extern crate atomic;
use atomic::Atomic;

extern crate rand;
use rand::Rng;

use std::collections::VecDeque;
use std::fmt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

use marauder::modes::draw::DrawMode;
use marauder::modes::touch::TouchMode;
use marauder::shapes::smileyface::smileyface;
use marauder::strokes::Stroke;

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
    static ref WACOM_UNDO: Mutex<Stroke> = Mutex::new(Stroke::default());
    static ref WACOM_UNDO_TICK: AtomicBool = AtomicBool::new(true);
    static ref G_COUNTER: Mutex<u32> = Mutex::new(0);
    static ref LAST_REFRESHED_CANVAS_RECT: Atomic<mxcfb_rect> = Atomic::new(mxcfb_rect::invalid());
    static ref SAVED_CANVAS: Mutex<Option<storage::CompressedCanvasState>> = Mutex::new(None);
}

// ####################
// ## Button Handlers
// ####################

fn on_undo(app: &mut ApplicationContext, _element: UIElementHandle) {
    let mut wacom_undo = WACOM_UNDO.lock().unwrap();
    wacom_undo.invert_color();
    wacom_undo.draw(app);
    wacom_undo.clear();
}

fn on_save_canvas(app: &mut ApplicationContext, _element: UIElementHandle) {
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

fn on_zoom_out(app: &mut ApplicationContext, _element: UIElementHandle) {
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

fn on_blur_canvas(app: &mut ApplicationContext, _element: UIElementHandle) {
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

fn on_invert_canvas(app: &mut ApplicationContext, element: UIElementHandle) {
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

fn on_load_canvas(app: &mut ApplicationContext, _element: UIElementHandle) {
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

fn on_toggle_eraser(app: &mut ApplicationContext, _: UIElementHandle) {
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

fn on_change_touchdraw_mode(app: &mut ApplicationContext, _: UIElementHandle) {
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

fn change_brush_width(app: &mut ApplicationContext, delta: i32) {
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

fn loop_update_topbar(app: &mut ApplicationContext, millis: u64) {
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

fn loop_companion(app: &mut ApplicationContext) {
    let mut strokes = smileyface(color::BLACK, Duration::from_millis(2));
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

fn on_wacom_input(app: &mut ApplicationContext, input: wacom::WacomEvent) {
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

fn on_touch_handler(app: &mut ApplicationContext, input: multitouch::MultitouchEvent) {
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

fn on_button_press(app: &mut ApplicationContext, input: gpio::GPIOEvent) {
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
    let mut app: ApplicationContext =
        ApplicationContext::new(on_button_press, on_wacom_input, on_touch_handler);

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
            position: (1155, 370).into(),
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
            position: (1247, 370).into(),
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
            position: (960, 440).into(),
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
            position: (1080, 440).into(),
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
            position: (1200, 440).into(),
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
            position: (960, 510).into(),
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
            position: (1210, 510).into(),
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
            position: (960, 580).into(),
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
            position: (1210, 580).into(),
            refresh: UIConstraintRefresh::Refresh,
            onclick: None,
            inner: UIElement::Text {
                foreground: color::BLACK,
                text: G_DRAW_MODE.load(Ordering::Relaxed).to_string(),
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
            position: (960, 670).into(),
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
            position: (1030, 670).into(),
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
            position: (1080, 670).into(),
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
            position: (1240, 670).into(),
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
            position: (1295, 670).into(),
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
            position: (30, 50).into(),
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
            position: (15, 1850).into(),
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
            position: (565, 1850).into(),
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
            position: (1112, 1850).into(),
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
            position: (30, 215).into(),
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
            position: (30, 150).into(),
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
