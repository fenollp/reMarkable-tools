use std::{
    collections::VecDeque,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
        LazyLock, Mutex,
    },
    thread::sleep,
    time::Duration,
};

use chrono::{DateTime, Local};
use libremarkable::{
    appctx::{self, ApplicationContext},
    battery,
    framebuffer::{
        cgmath::{self, EuclideanSpace},
        common::*,
        storage::{self, CompressedCanvasState},
        FramebufferDraw, FramebufferIO, FramebufferRefresh, PartialRefreshMode,
    },
    image::{self, GenericImage},
    input::{
        GPIOEvent, InputDevice, InputEvent, MultitouchEvent, PhysicalButton, WacomEvent, WacomPen,
    },
    ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementHandle, UIElementWrapper},
};
use log::{debug, error, info};
use marauder::{
    modes::{draw::DrawMode, touch::TouchMode},
    strokes::Strokes,
    unipen,
};

// This region will have the following size at rest:
//   raw: 5896 kB
//   zstd: 10 kB
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 720,
    left: 0,
    height: 1080 + 50, //1850? 1900? !1872
    width: 1404,
};

type PosNpress = (cgmath::Point2<f32>, i32); // position and pressure

static G_TOUCH_MODE: LazyLock<AtomicU8> = LazyLock::new(|| AtomicU8::new(TouchMode::OnlyUI.into()));
static G_DRAW_MODE: LazyLock<AtomicI32> = LazyLock::new(Default::default);
static UNPRESS_OBSERVED: LazyLock<AtomicBool> = LazyLock::new(Default::default);
static WACOM_IN_RANGE: LazyLock<AtomicBool> = LazyLock::new(Default::default);
static WACOM_HISTORY: LazyLock<Mutex<VecDeque<PosNpress>>> = LazyLock::new(Default::default);
static DRAWING: LazyLock<AtomicBool> = LazyLock::new(Default::default);
static SAVED_CANVAS: LazyLock<Mutex<Option<CompressedCanvasState>>> =
    LazyLock::new(Default::default);
static SAVED_CANVAS_PREV: LazyLock<Mutex<Option<CompressedCanvasState>>> =
    LazyLock::new(Default::default);

// ####################
// ## Button Handlers
// ####################

fn on_undo(app: &mut ApplicationContext, _: UIElementHandle) {
    let mut undone = false;
    {
        let mut prev = SAVED_CANVAS_PREV.lock().unwrap();
        if let Some(ref compressed_state) = *prev {
            let decompressed = compressed_state.decompress();
            let framebuffer = app.get_framebuffer_ref();
            // TODO: restore & refresh only the subset region that was just drawn onto
            match framebuffer.restore_region(CANVAS_REGION, &decompressed) {
                Err(e) => error!("Error while restoring region: {e}"),
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
                    *prev = None;
                    *SAVED_CANVAS.lock().unwrap() = None;
                    undone = true;
                }
            };
        }
    }
    // Seprate scopes to avoid deadlocks
    if undone {
        save_canvas(app);
    }
}

fn save_canvas(app: &mut ApplicationContext) {
    let framebuffer = app.get_framebuffer_ref();
    match framebuffer.dump_region(CANVAS_REGION) {
        Err(err) => error!("Failed to dump buffer: {0}", err),
        Ok(buff) => {
            let mut hist = SAVED_CANVAS.lock().unwrap();
            if let Some(ref compressed_state) = *hist {
                let mut prev = SAVED_CANVAS_PREV.lock().unwrap();
                *prev = Some((*compressed_state).clone());
            }
            *hist = Some(CompressedCanvasState::new(
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
            new_image
                .copy_from(&resized, CANVAS_REGION.width / 8, CANVAS_REGION.height / 8)
                .unwrap();

            framebuffer
                .draw_image(new_image.as_rgb8().unwrap(), CANVAS_REGION.top_left().cast().unwrap());
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

            framebuffer
                .draw_image(dynamic.as_rgb8().unwrap(), CANVAS_REGION.top_left().cast().unwrap());
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

fn on_toggle_eraser(app: &mut ApplicationContext, _: UIElementHandle) {
    let (new_mode, next_name) = match DrawMode::from(G_DRAW_MODE.load(Ordering::Relaxed)) {
        DrawMode::Erase(s) => (DrawMode::Draw(s), "Switch to BLACK".to_owned()),
        DrawMode::Draw(s) => (DrawMode::Erase(s), "Switch to WHITE".to_owned()),
    };
    G_DRAW_MODE.store(new_mode.into(), Ordering::Relaxed);

    let element = app.get_element_by_name("colorToggle").unwrap();
    if let UIElement::Text { ref mut text, .. } = element.write().inner {
        *text = next_name;
    }
    app.draw_element("colorToggle");
}

fn on_tap_touchdraw_mode(app: &mut ApplicationContext, _: UIElementHandle) {
    let new_val = TouchMode::from(G_TOUCH_MODE.load(Ordering::Relaxed)).toggle();
    G_TOUCH_MODE.store(new_val.into(), Ordering::Relaxed);

    let element = app.get_element_by_name("touchdrawMode").unwrap();
    if let UIElement::Text { ref mut text, .. } = element.write().inner {
        *text = new_val.to_string();
    }
    // Make sure you aren't trying to draw the element while you are holding a write lock.
    // It doesn't seem to cause a deadlock however it may cause higher lock contention.
    app.draw_element("touchdrawMode");
}

// ####################
// ## Miscellaneous
// ####################

fn change_brush_width(app: &mut ApplicationContext, delta: i32) {
    let current = DrawMode::from(G_DRAW_MODE.load(Ordering::Relaxed));
    let current_size = current.get_size() as i32;
    let proposed_size = current_size + delta;
    let new_size = proposed_size.clamp(1, 99);
    if new_size == current_size {
        return;
    }

    G_DRAW_MODE.store(current.set_size(new_size as u32).into(), Ordering::Relaxed);

    let element = app.get_element_by_name("displaySize").unwrap();
    if let UIElement::Text { ref mut text, .. } = element.write().inner {
        *text = format!("size: {0}", new_size);
    }
    app.draw_element("displaySize");
}

fn loop_update_battime(app: &mut ApplicationContext) {
    let element = app.get_element_by_name("battime").unwrap();
    loop {
        if let UIElement::Text { ref mut text, .. } = element.write().inner {
            let now = (Local::now() as DateTime<Local>).format("%F %r");
            let status = battery::human_readable_charging_status().unwrap();
            let percents = battery::percentage().unwrap();
            *text = format!("{}% ~ {:<80} {}", percents, status, now);
        }
        app.draw_element("battime");
        sleep(Duration::from_millis(37_000));
    }
}

fn loop_companion(app: &mut ApplicationContext) {
    // let mut strokes = smileyface::abs(color::BLACK, Duration::from_millis(2));
    // let (xmin, xmax, ymin, ymax) = strokes.translation_boundaries();
    // let mut rng = rand::thread_rng();

    // loop {
    //     // select (dx,dy) such that strokes shifted by (dx,dy) is still within CANVAS_REGION
    //     let dx = rng.gen_range(xmin, xmax);
    //     let dy = rng.gen_range(ymin, ymax);
    //     strokes.translate((dx, dy));
    //     strokes.draw(app);
    //     sleep(Duration::from_millis(100));
    // }

    if let Ok((_, words)) = unipen::words(include_str!("../../ujipenchars2.txt")) {
        debug!("Loaded {} glyphs", words.len());
        for word in &words {
            let glyph = Strokes::from_ujipenchars(word);
            glyph.draw(app);
            sleep(Duration::from_millis(100));
        }
    }
}

// ####################
// ## Input Handlers
// ####################

fn on_wacom_input(app: &mut ApplicationContext, input: WacomEvent) {
    match input {
        WacomEvent::Draw { position, pressure, tilt: _ } => {
            // debug!("{} {} {}", position.x, position.y, pressure);

            let mut wacom_stack = WACOM_HISTORY.lock().unwrap();

            // Outside of drawable region
            if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
                // This is so that we can click the buttons outside the canvas region
                // normally meant to be touched with a finger using our stylus
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

            if !DRAWING.load(Ordering::Relaxed) {
                // Started drawing
                DRAWING.store(true, Ordering::Relaxed);
            }

            let (col, mult) = match G_DRAW_MODE.load(Ordering::Relaxed).into() {
                DrawMode::Draw(s) => (color::BLACK, s),
                DrawMode::Erase(s) => (color::WHITE, s * 3),
            };

            wacom_stack.push_back((position.cast().unwrap(), i32::from(pressure)));
            while wacom_stack.len() >= 3 {
                let framebuffer = app.get_framebuffer_ref();
                let points = [
                    wacom_stack.pop_front().unwrap(),
                    *wacom_stack.front().unwrap(),
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
        WacomEvent::InstrumentChange { pen, state } => {
            match pen {
                WacomPen::ToolPen => {
                    // Whether the pen is in range
                    let in_range = state;
                    WACOM_IN_RANGE.store(in_range, Ordering::Relaxed);
                }
                WacomPen::Touch => {
                    // Whether the pen is actually making contact
                    let making_contact = state;
                    if !making_contact {
                        let was_just_drawing = DRAWING.fetch_and(false, Ordering::Relaxed);
                        if was_just_drawing {
                            save_canvas(app);
                        }
                        let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                        wacom_stack.clear();
                    }
                }
                _ => unreachable!(),
            }
        }
        WacomEvent::Hover { position: _, distance, tilt: _ } => {
            // If the pen is hovering, don't record its coordinates as the origin of the next line
            if distance > 1 {
                let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                wacom_stack.clear();
                UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    };
}

fn on_touch_handler(app: &mut ApplicationContext, input: MultitouchEvent) {
    let framebuffer = app.get_framebuffer_ref();
    if let MultitouchEvent::Press { finger } | MultitouchEvent::Move { finger } = input {
        let position = finger.pos;
        if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
            return;
        }
        let rect = match G_TOUCH_MODE.load(Ordering::Relaxed).into() {
            TouchMode::Bezier => {
                let position_float = position.cast().unwrap();
                let points = [
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

fn on_button_press(app: &mut ApplicationContext, input: GPIOEvent) {
    let (btn, new_state) = match input {
        GPIOEvent::Press { button } => (button, true),
        GPIOEvent::Unpress { button } => (button, false),
        GPIOEvent::Unknown => return,
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
        PhysicalButton::RIGHT => {
            let new_state = if app.is_input_device_active(InputDevice::Multitouch) {
                app.deactivate_input_device(InputDevice::Multitouch);
                "Enable Touch"
            } else {
                app.activate_input_device(InputDevice::Multitouch);
                "Disable Touch"
            };

            if let Some(ref elem) = app.get_element_by_name("tooltipRight") {
                if let UIElement::Text { ref mut text, scale: _, foreground: _, border_px: _ } =
                    elem.write().inner
                {
                    new_state.clone_into(text);
                }
            }
            app.draw_element("tooltipRight");
        }
        PhysicalButton::MIDDLE | PhysicalButton::LEFT => {
            app.clear(btn == PhysicalButton::MIDDLE);
            app.draw_elements();
        }
        PhysicalButton::POWER => {
            Command::new("systemctl").arg("start").arg("xochitl").spawn().unwrap();
            std::process::exit(0);
        }
        PhysicalButton::WAKEUP => {
            info!("WAKEUP button(?) pressed(?)");
        }
    };
}

fn main() {
    env_logger::init();

    // Takes callback functions as arguments
    // They are called with the event and the &mut framebuffer
    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();

    // Alternatively we could have called `app.execute_lua("fb.clear()")`
    app.clear(true);

    // Draw the borders for the canvas region
    app.add_element(
        "canvasRegion",
        UIElementWrapper {
            position: CANVAS_REGION.top_left().cast().unwrap() + cgmath::vec2(0, -2),
            refresh: UIConstraintRefresh::RefreshAndWait,
            inner: UIElement::Region {
                size: CANVAS_REGION.size().cast().unwrap() + cgmath::vec2(1, 3),
                border_px: 2,
                border_color: color::BLACK,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "zoomoutButton",
        UIElementWrapper {
            position: (960, 370).into(),
            onclick: Some(on_zoom_out),
            inner: UIElement::Text {
                text: "Zoom Out".to_owned(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "blurToggle",
        UIElementWrapper {
            position: (1155, 370).into(),
            onclick: Some(on_blur_canvas),
            inner: UIElement::Text {
                text: "Blur".to_owned(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "invertToggle",
        UIElementWrapper {
            position: (1247, 370).into(),
            onclick: Some(on_invert_canvas),
            inner: UIElement::Text {
                text: "Invert".to_owned(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "undoButton",
        UIElementWrapper {
            position: (30, 130).into(),
            onclick: Some(on_undo),
            inner: UIElement::Text {
                text: "Undo".to_owned(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 90.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "touchdrawMode",
        UIElementWrapper {
            position: (960, 510).into(),
            onclick: Some(on_tap_touchdraw_mode),
            inner: UIElement::Text {
                text: "Touch Mode".to_owned(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "colorToggle",
        UIElementWrapper {
            position: (960, 580).into(),
            onclick: Some(on_toggle_eraser),
            inner: UIElement::Text {
                text: DrawMode::default().to_string(),
                border_px: 5,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "decreaseSizeSkip",
        UIElementWrapper {
            position: (960, 670).into(),
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, -10);
            }),
            inner: UIElement::Text {
                text: "--".to_owned(),
                scale: 90.0,
                border_px: 5,
                foreground: color::BLACK,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "decreaseSize",
        UIElementWrapper {
            position: (1030, 670).into(),
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, -1);
            }),
            inner: UIElement::Text {
                text: "-".to_owned(),
                scale: 90.0,
                border_px: 5,
                foreground: color::BLACK,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "displaySize",
        UIElementWrapper {
            position: (1080, 670).into(),
            inner: UIElement::Text {
                text: format!(
                    "size: {0}",
                    DrawMode::from(G_DRAW_MODE.load(Ordering::Relaxed)).get_size()
                ),
                border_px: 0,
                foreground: color::BLACK,
                scale: 45.0,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "increaseSize",
        UIElementWrapper {
            position: (1240, 670).into(),
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, 1);
            }),
            inner: UIElement::Text {
                text: "+".to_owned(),
                scale: 60.0,
                border_px: 5,
                foreground: color::BLACK,
            },
            ..Default::default()
        },
    );
    app.add_element(
        "increaseSizeSkip",
        UIElementWrapper {
            position: (1295, 670).into(),
            onclick: Some(|appctx, _| {
                change_brush_width(appctx, 10);
            }),
            inner: UIElement::Text {
                text: "++".to_owned(),
                scale: 60.0,
                border_px: 5,
                foreground: color::BLACK,
            },
            ..Default::default()
        },
    );

    app.add_element(
        "battime",
        UIElementWrapper {
            position: (30, 50).into(),
            inner: UIElement::Text {
                text: "Press POWER to return to reMarkable".to_owned(),
                scale: 40.0,
                foreground: color::BLACK,
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
        loop_update_battime(appref);
    });

    let appref2 = app.upgrade_ref();
    let companion_thread = std::thread::spawn(move || {
        loop_companion(appref2);
    });

    info!("Init complete. Beginning event dispatch...");

    // Blocking call to process events from digitizer + touchscreen + physical buttons
    app.start_event_loop(true, true, true, |ctx, evt| match evt {
        InputEvent::WacomEvent { event } => on_wacom_input(ctx, event),
        InputEvent::MultitouchEvent { event } => on_touch_handler(ctx, event),
        InputEvent::GPIO { event } => on_button_press(ctx, event),
        InputEvent::Unknown {} => {}
    });
    clock_thread.join().unwrap();
    companion_thread.join().unwrap();
}
