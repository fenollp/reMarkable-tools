#![feature(nll)]
#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate env_logger;

use atomic::Atomic;
use chrono::DateTime;
use chrono::Local;
use libremarkable::appctx::ApplicationContext;
use libremarkable::battery;
use libremarkable::framebuffer::cgmath;
use libremarkable::framebuffer::cgmath::EuclideanSpace;
use libremarkable::framebuffer::common::*;
use libremarkable::framebuffer::refresh::PartialRefreshMode;
use libremarkable::framebuffer::storage;
use libremarkable::framebuffer::FramebufferDraw;
use libremarkable::framebuffer::FramebufferIO;
use libremarkable::framebuffer::FramebufferRefresh;
use libremarkable::input::gpio;
use libremarkable::input::multitouch;
use libremarkable::input::wacom;
use libremarkable::input::InputDevice;
use libremarkable::ui_extensions::element::UIConstraintRefresh;
use libremarkable::ui_extensions::element::UIElement;
use libremarkable::ui_extensions::element::UIElementHandle;
use libremarkable::ui_extensions::element::UIElementWrapper;
use marauder::modes::draw::DrawMode;
use marauder::modes::touch::TouchMode;
use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread::sleep;
use std::time::Duration;

// This region will have the following size at rest:
//   raw: 5896 kB
//   zstd: 10 kB
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 720,
    left: 0,
    height: 1080 + 50, //1850? 1900? !1872
    width: 1404,
};

lazy_static! {
    static ref G_TOUCH_MODE: Atomic<TouchMode> = Atomic::new(TouchMode::OnlyUI);
    static ref G_DRAW_MODE: Atomic<DrawMode> = Atomic::new(DrawMode::default());
    static ref UNPRESS_OBSERVED: AtomicBool = AtomicBool::new(false);
    static ref WACOM_IN_RANGE: AtomicBool = AtomicBool::new(false);
    static ref WACOM_HISTORY: Mutex<VecDeque<(cgmath::Point2<f32>, i32)>> =
        Mutex::new(VecDeque::new());
    static ref DRAWING: AtomicBool = AtomicBool::new(false);
    static ref SAVED_CANVAS: Mutex<Option<storage::CompressedCanvasState>> = Mutex::new(None);
    static ref SAVED_CANVAS_PREV: Mutex<Option<storage::CompressedCanvasState>> = Mutex::new(None);
    static ref TX: Mutex<Option<std::sync::mpsc::Sender<Drawing>>> = Mutex::new(None);
}

// ####################
// ## Button Handlers
// ####################

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
            *hist = Some(storage::CompressedCanvasState::new(
                buff.as_slice(),
                CANVAS_REGION.height,
                CANVAS_REGION.width,
            ));
        }
    };
}

fn on_toggle_eraser(app: &mut ApplicationContext, _: UIElementHandle) {
    let (new_mode, next_name) = match G_DRAW_MODE.load(Ordering::Relaxed) {
        DrawMode::Erase(s) => (DrawMode::Draw(s), "Switch to BLACK".to_owned()),
        DrawMode::Draw(s) => (DrawMode::Erase(s), "Switch to WHITE".to_owned()),
    };
    G_DRAW_MODE.store(new_mode, Ordering::Relaxed);

    let element = app.get_element_by_name("colorToggle").unwrap();
    if let UIElement::Text { ref mut text, .. } = element.write().inner {
        *text = next_name;
    }
    app.draw_element("colorToggle");
}

// ####################
// ## Miscellaneous
// ####################

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

// ####################
// ## Input Handlers
// ####################

fn on_pen(app: &mut ApplicationContext, input: wacom::WacomEvent) {
    match input {
        wacom::WacomEvent::Draw {
            position,
            pressure,
            tilt: _,
        } => {
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

            let (col, mult) = match G_DRAW_MODE.load(Ordering::Relaxed) {
                DrawMode::Draw(s) => (color::BLACK, s),
                DrawMode::Erase(s) => (color::WHITE, s * 3),
            };

            wacom_stack.push_back((position.cast().unwrap(), pressure as i32));
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
                wacom::WacomPen::ToolPen => {
                    // Whether the pen is in range
                    let in_range = state;
                    WACOM_IN_RANGE.store(in_range, Ordering::Relaxed);
                }
                wacom::WacomPen::Touch => {
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
        wacom::WacomEvent::Hover {
            position: _,
            distance,
            tilt: _,
        } => {
            // If the pen is hovering, don't record its coordinates as the origin of the next line
            if distance > 1 {
                let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                let drawing = Drawing {};
                wacom_stack.clear();

                // FIXME: gRPC call Whiteboard/SendEvent(hist)
                let wtx = TX.lock().unwrap();
                if let Some(ref tx) = *wtx {
                    tx.send(drawing).unwrap();
                }

                UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    };
}

fn on_tch(app: &mut ApplicationContext, input: multitouch::MultitouchEvent) {
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

fn on_btn(app: &mut ApplicationContext, input: gpio::GPIOEvent) {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Takes callback functions as arguments
    // They are called with the event and the &mut framebuffer
    let mut app: ApplicationContext = ApplicationContext::new(on_btn, on_pen, on_tch);

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
        "displaySize",
        UIElementWrapper {
            position: (1080, 670).into(),
            inner: UIElement::Text {
                text: format!("size: {0}", G_DRAW_MODE.load(Ordering::Relaxed).get_size()),
                border_px: 0,
                foreground: color::BLACK,
                scale: 45.0,
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

    let appref1 = app.upgrade_ref();
    let appref2 = app.upgrade_ref();

    info!("Connecting...");
    let channel = Endpoint::from_static("http://[::1]:10000")
        .connect()
        .await
        .unwrap();
    let mut client1 = WhiteboardClient::new(channel.clone());
    let mut client2 = WhiteboardClient::new(channel);

    tokio::spawn(async move {
        loop_update_battime(appref1);
    });
    tokio::spawn(async move {
        loop_recv(appref2, &mut client2).await;
    });

    info!("Init complete. Beginning event dispatch...");

    tokio::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut wtx = TX.lock().unwrap();
            *wtx = Some(tx.to_owned());
        }
        loop {
            let rcvd = rx.recv();
            match rcvd {
                Ok(drawing) => send_drawing(&mut client1, drawing).await,
                Err(e) => error!("!rx {:?}", e),
            }
        }
    });

    // Blocking call to process events from digitizer + touchscreen + physical buttons
    app.dispatch_events(true, true, true);

    Ok(())
}

use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tonic::Request;

use tokio::time::delay_for;

use whiteboard::whiteboard_client::WhiteboardClient;
use whiteboard::{Drawing, Event, RecvEventsReq, SendEventReq};

pub mod whiteboard {
    tonic::include_proto!("hypercard.whiteboard");
}

fn add_xuser<T>(req: &mut Request<T>) {
    let user_id = "Joe".parse().unwrap();
    let md = Request::metadata_mut(req);
    assert!(md.insert("x-user", user_id).is_none());
}

async fn loop_recv(app: &mut ApplicationContext<'_>, client: &mut WhiteboardClient<Channel>) {
    let mut req = Request::new(RecvEventsReq {
        room_id: "living-room".into(),
    });
    add_xuser(&mut req);
    info!("Receiving... {:?}", req);

    let mut stream = client.recv_events(req).await.unwrap().into_inner();

    while let Some(event) = stream.message().await.unwrap() {
        println!("EVENT = {:?}", event);
        if let Some(drawing) = event.event_drawing {
            delay_for(Duration::from_secs(1)).await;
            debug!(
                "MT? {:?}",
                app.is_input_device_active(InputDevice::Multitouch)
            );
        }
    }
}

async fn send_drawing(client: &mut WhiteboardClient<Channel>, drawing: Drawing) {
    let mut req = Request::new(SendEventReq {
        event: Some(Event {
            created_at: 0,
            user_id: "".into(),
            room_id: "".into(),
            event_drawing: Some(drawing),
            event_user_left_the_room: true,
            event_user_joined_the_room: false,
        }),
        room_ids: vec!["living-room".into()],
    });
    add_xuser(&mut req);
    info!("REQ = {:?}", req);
    let rep = client
        .send_event(req)
        .await
        .map_err(|e| error!("!Send: {:?}", e));
    info!("REP = {:?}", rep);
}
