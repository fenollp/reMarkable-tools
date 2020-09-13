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
use libremarkable::framebuffer::FramebufferDraw;
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
    static ref STROKES: Mutex<Vec<(color, u32, f32, f32, i32)>> = Mutex::new(Vec::new());
    static ref TX: Mutex<Option<std::sync::mpsc::Sender<Drawing>>> = Mutex::new(None);
}

// ####################
// ## Button Handlers
// ####################

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

async fn loop_update_battime(app: &mut ApplicationContext<'_>) {
    let element = app.get_element_by_name("battime").unwrap();
    loop {
        if let UIElement::Text { ref mut text, .. } = element.write().inner {
            let now = (Local::now() as DateTime<Local>).format("%F %r");
            let status = battery::human_readable_charging_status().unwrap();
            let percents = battery::percentage().unwrap();
            *text = format!("{}% ~ {:<80} {}", percents, status, now);
        }
        app.draw_element("battime");
        delay_for(Duration::from_millis(37_000)).await;
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
            let mut wacom_stack = WACOM_HISTORY.lock().unwrap();

            // Outside of drawable region
            if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
                // This is so that we can click the buttons outside the canvas region
                // normally meant to be touched with a finger using our stylus
                wacom_stack.clear();
                maybe_send_drawing();
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

            {
                let mut strokes = STROKES.lock().unwrap();
                strokes.push((col, mult, position.x, position.y, pressure as i32));
            }

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
                        let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                        wacom_stack.clear();
                        maybe_send_drawing();
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
                maybe_send_drawing();

                UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
            }
        }
        _ => {}
    };
}

fn maybe_send_drawing() {
    let mut strokes = STROKES.lock().unwrap();
    let len = strokes.len();
    if len < 3 {
        return;
    }
    debug!("strokes.len() = {:?}", len);

    let mut ws = Vec::<u32>::new();
    let mut xs = Vec::<f32>::new();
    let mut ys = Vec::<f32>::new();
    let mut ps = Vec::<i32>::new();
    ws.reserve(len);
    xs.reserve(len);
    ys.reserve(len);
    ps.reserve(len);
    for i in 0..len {
        let dot = strokes[i];
        ws.push(dot.1);
        xs.push(dot.2);
        ys.push(dot.3);
        ps.push(dot.4);
    }

    let col = match strokes[0].0 {
        color::WHITE => Color::White,
        _ => Color::Black,
    };

    debug!("locking TX");
    let wtx = TX.lock().unwrap();
    match &*wtx {
        Some(ref tx) => {
            let drawing = Drawing {
                xs,
                ys,
                pressures: ps,
                widths: ws,
                color: col as i32,
            };
            tx.send(drawing).unwrap();
        }
        e => error!("e = {:?}", e),
    };
    debug!("unlocked TX");
    strokes.clear();
}

fn on_tch(_app: &mut ApplicationContext, input: multitouch::MultitouchEvent) {
    if let multitouch::MultitouchEvent::Press { finger }
    | multitouch::MultitouchEvent::Move { finger } = input
    {
        let position = finger.pos;
        if !CANVAS_REGION.contains_point(&position.cast().unwrap()) {
            return;
        }
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

use docopt::Docopt;
use serde::Deserialize;

const USAGE: &'static str = "
reMarkable whiteboard HyperCard.

Usage:
  whiteboard [--host=<HOST>] [--user=USER] [--room=<ROOM>]
  whiteboard (-h | --help)
  whiteboard --version

Options:
  --host=<HOST>  Server to connect to [default: http://fknwkdacd.com:10000].
  --user=<USER>  User to connect as [default: Jane].
  --room=<ROOM>  Room to join [default: living-room].
  -h --help      Show this screen.
  --version      Show version.
";

#[derive(Debug, Deserialize, Clone)]
struct Args {
    flag_host: String,
    flag_user: String,
    flag_room: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    debug!("{:?}", args);

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
    tokio::spawn(async move {
        loop_update_battime(appref1).await;
    });

    let host = args.clone().flag_host;
    info!("[main] connecting to {:?}...", host);
    let channel1 = Endpoint::from_shared(host)
        .unwrap()
        // .timeout(Duration::from_secs(5))
        .connect()
        .await?;
    // FIXME: use only one connection
    let mut client1 = WhiteboardClient::new(channel1);
    let channel2 = Endpoint::from_shared(args.clone().flag_host)
        .unwrap()
        .connect()
        .await?;
    let mut client2 = WhiteboardClient::new(channel2);

    let args2 = args.clone();
    let appref2 = app.upgrade_ref();
    info!("[loop_recv] spawn-ing");
    tokio::spawn(async move {
        info!("[loop_recv] spawn-ed");
        loop_recv(appref2, &mut client2, args2).await;
        info!("[loop_recv] terminated");
    });

    info!("[TXer] spawn-ing");
    tokio::spawn(async move {
        info!("[TXer] spawn-ed");
        let (tx, rx) = std::sync::mpsc::channel();
        {
            info!("[TXer] locking");
            let mut wtx = TX.lock().unwrap();
            *wtx = Some(tx.to_owned());
            info!("[TXer] unlocked");
        }
        loop {
            let rcvd = rx.recv();
            debug!("[TXer] FWDing...");
            match rcvd {
                Ok(drawing) => send_drawing(&mut client1, drawing, &args).await,
                Err(e) => error!("[TXer] failed to FWD: {:?}", e),
            }
            tokio::task::yield_now().await;
        }
    });

    info!("Init complete. Beginning event dispatch...");
    // Blocking call to process events from digitizer + touchscreen + physical buttons
    app.dispatch_events(true, true, true);

    Ok(())
}

use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tonic::Request;

use tokio::time::delay_for;

use whiteboard::drawing::Color;
use whiteboard::whiteboard_client::WhiteboardClient;
use whiteboard::{Drawing, Event, RecvEventsReq, SendEventReq};

pub mod whiteboard {
    tonic::include_proto!("hypercard.whiteboard");
}

fn add_xuser<T>(req: &mut Request<T>, user: String) {
    let user_id = user.parse().unwrap();
    let md = Request::metadata_mut(req);
    assert!(md.insert("x-user", user_id).is_none());
}

async fn loop_recv(
    app: &mut ApplicationContext<'_>,
    client: &mut WhiteboardClient<Channel>,
    args: Args,
) {
    let mut req = Request::new(RecvEventsReq {
        room_id: args.flag_room,
    });
    add_xuser(&mut req, args.flag_user);

    info!("[loop_recv] creating stream");
    let mut stream = client.recv_events(req).await.unwrap().into_inner();
    info!("[loop_recv] receiving...");

    while let Some(event) = stream.message().await.unwrap() {
        debug!("[loop_recv] received event {:?}", event);

        if let Some(drawing) = event.event_drawing {
            let col = match drawing.color() {
                Color::White => color::WHITE,
                _ => color::BLACK,
            };
            let (xs, ys, ps, ws) = (drawing.xs, drawing.ys, drawing.pressures, drawing.widths);
            let len = xs.len();
            debug!("[loop_recv] xs.len() = {:?}", len);
            if len < 3 {
                continue;
            }
            for i in 0..len - 2 {
                let points: Vec<(cgmath::Point2<f32>, i32, u32)> = vec![
                    // start
                    (
                        cgmath::Point2 {
                            x: xs[i + 0],
                            y: ys[i + 0],
                        },
                        ps[i + 0],
                        ws[i + 0],
                    ),
                    // ctrl
                    (
                        cgmath::Point2 {
                            x: xs[i + 1],
                            y: ys[i + 1],
                        },
                        ps[i + 1],
                        ws[i + 1],
                    ),
                    // end
                    (
                        cgmath::Point2 {
                            x: xs[i + 2],
                            y: ys[i + 2],
                        },
                        ps[i + 2],
                        ws[i + 2],
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
                    DRAWING_QUANT_BIT,
                    false,
                );
                delay_for(Duration::from_millis(2)).await;
            }
            info!("[loop_recv] painted");
        }
        // tokio::task::yield_now().await;
    }
}

async fn send_drawing(client: &mut WhiteboardClient<Channel>, drawing: Drawing, args: &Args) {
    let mut req = Request::new(SendEventReq {
        event: Some(Event {
            created_at: 0,
            user_id: "".into(),
            room_id: "".into(),
            event_drawing: Some(drawing),
            event_user_left_the_room: false,
            event_user_joined_the_room: false,
        }),
        room_ids: vec![args.flag_room.to_owned()],
    });
    add_xuser(&mut req, args.flag_user.to_owned());
    info!("REQ = {:?}", req);
    let rep = client
        .send_event(req)
        .await
        .map_err(|e| error!("!Send: {:?}", e));
    info!("REP = {:?}", rep);
}
