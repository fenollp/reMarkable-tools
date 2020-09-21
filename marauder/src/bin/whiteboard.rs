use docopt::Docopt;
use lazy_static::lazy_static;
use libremarkable::appctx::ApplicationContext;
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
use libremarkable::ui_extensions::element::UIElementWrapper;
use log::{debug, error, info};
use marauder::modes::draw::DrawMode;
use serde::Deserialize;
use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::delay_for;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tonic::Request;
use uuid::Uuid;
use whiteboard::whiteboard_client::WhiteboardClient;
use whiteboard::{drawing, event};
use whiteboard::{Drawing, Event};
use whiteboard::{RecvEventsReq, SendEventReq};

pub mod whiteboard {
    tonic::include_proto!("hypercard.whiteboard");
}

const USAGE: &str = "
reMarkable whiteboard HyperCard.

Usage:
  whiteboard [--room=<ROOM>] [--host=<HOST>]
  whiteboard (-h | --help)
  whiteboard --version

Options:
  --host=<HOST>  Server to connect to [default: http://fknwkdacd.com:10000].
  --room=<ROOM>  Room to join [default: living-room].
  -h --help      Show this screen.
  --version      Show version.
";

#[derive(Debug, Deserialize, Clone)]
struct Args {
    flag_host: String,
    flag_room: String,
}

#[derive(Debug, Clone)]
struct Ctx {
    args: Args,
    user_id: String,
}

const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 45,
    left: 0,
    height: 45 + 90 + 180 + 360 + 1080 + 50, //1850? 1900? !1872
    width: 1404,
};

lazy_static! {
    static ref UNPRESS_OBSERVED: AtomicBool = AtomicBool::new(false);
    static ref WACOM_IN_RANGE: AtomicBool = AtomicBool::new(false);
    static ref WACOM_HISTORY: Mutex<VecDeque<(cgmath::Point2<f32>, i32)>> =
        Mutex::new(VecDeque::new());
    static ref STROKES: Mutex<Vec<(color, u32, f32, f32, i32)>> = Mutex::new(Vec::new());
    static ref TX: Mutex<Option<std::sync::mpsc::Sender<Drawing>>> = Mutex::new(None);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    debug!("args = {:?}", args);

    let user_id = Uuid::new_v4().to_hyphenated().to_string();
    debug!("user_id = {:?}", user_id);
    // TODO: save settings under ~/.hypercards/whiteboard/<user_id>/...
    // https://github.com/whitequark/rust-xdg
    // ...but does $HOME survives xochitl updates?

    // TODO: check for updates when asked:
    // reqwest JSON API equivalent of https://github.com/fenollp/reMarkable-tools/releases
    // and select the highest semver these
    // DL + decompress + checksum + chmod + move + execve
    // unless version is current

    let mut app: ApplicationContext = ApplicationContext::new(on_btn, on_pen, on_tch);
    app.clear(true);

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

    app.draw_elements();

    let host = args.clone().flag_host;
    info!("[main] connecting to {:?}...", host);
    let ch = Endpoint::from_shared(host).unwrap().connect().await?;

    let ch2 = ch.clone();
    let ctx2 = Ctx {
        args: args.clone(),
        user_id: user_id.clone(),
    };
    let appref = app.upgrade_ref();
    info!("[loop_recv] spawn-ing");
    tokio::task::spawn_blocking(move || {
        //TODO: PR to allow async in spawn_blocking
        let rt_handle = tokio::runtime::Handle::current();
        rt_handle.block_on(async move {
            info!("[loop_recv] spawn-ed");
            loop_recv(appref, ch2, ctx2).await;
            info!("[loop_recv] terminated");
        });
    });

    info!("[TXer] spawn-ing");
    tokio::task::spawn_blocking(move || {
        let rt_handle = tokio::runtime::Handle::current();
        rt_handle.block_on(async move {
            info!("[TXer] spawn-ed");
            let (tx, rx) = std::sync::mpsc::channel();
            //TODO: let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            {
                info!("[TXer] locking");
                let mut wtx = TX.lock().unwrap();
                *wtx = Some(tx.to_owned());
                info!("[TXer] unlocked");
            }
            let ctx = Ctx { args, user_id };
            let mut client = WhiteboardClient::new(ch);
            loop {
                let rcvd = rx.recv();
                debug!("[TXer] FWDing...");
                match rcvd {
                    Ok(drawing) => send_drawing(&mut client, drawing, &ctx).await,
                    Err(e) => error!("[TXer] failed to FWD: {:?}", e),
                }
            }
        });
    });

    info!("Init complete. Beginning event dispatch...");
    app.dispatch_events(true, true, true);

    Ok(())
}

fn on_pen(app: &mut ApplicationContext, input: wacom::WacomEvent) {
    match input {
        wacom::WacomEvent::Draw {
            position,
            pressure,
            tilt: _,
        } => {
            let mut wacom_stack = WACOM_HISTORY.lock().unwrap();

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

            let (col, mult) = (color::BLACK, DrawMode::default().get_size());

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
        color::WHITE => drawing::Color::White,
        _ => drawing::Color::Black,
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

fn on_tch(_app: &mut ApplicationContext, _input: multitouch::MultitouchEvent) {}

fn on_btn(app: &mut ApplicationContext, input: gpio::GPIOEvent) {
    let (btn, pressed) = match input {
        gpio::GPIOEvent::Press { button } => (button, true),
        gpio::GPIOEvent::Unpress { button } => (button, false),
        _ => return,
    };

    // Ignoring the unpressed event
    if !pressed {
        return;
    }

    // Simple but effective accidental button press filtering
    if WACOM_IN_RANGE.load(Ordering::Relaxed) {
        return;
    }

    match btn {
        gpio::PhysicalButton::RIGHT => {
            if app.is_input_device_active(InputDevice::Multitouch) {
                app.deactivate_input_device(InputDevice::Multitouch);
            }
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

fn add_xuser<T>(req: &mut Request<T>, user: String) {
    let user_id = user.parse().unwrap();
    let md = Request::metadata_mut(req);
    assert!(md.insert("x-user", user_id).is_none());
}

async fn loop_recv(app: &mut ApplicationContext<'_>, ch: Channel, ctx: Ctx) {
    let mut req = Request::new(RecvEventsReq {
        room_id: ctx.args.flag_room,
    });
    add_xuser(&mut req, ctx.user_id);

    info!("[loop_recv] creating stream");
    let mut client = WhiteboardClient::new(ch);
    let mut stream = client.recv_events(req).await.unwrap().into_inner();
    info!("[loop_recv] receiving...");

    loop {
        let msg = stream.message().await.unwrap();
        match msg {
            None => error!("[loop_recv] empty message in gRPC stream"),
            Some(event) => match event.event {
                None => error!("[loop_recv] empty event in message"),
                Some(event::Event::Drawing(drawing)) => {
                    let len = drawing.xs.len();
                    if len < 3 {
                        continue;
                    }
                    debug!("[loop_recv] drawing {:?} points", len - 2);
                    paint(app, drawing).await;
                    info!("[loop_recv] painted");
                }
                Some(event::Event::UserJoinedTheRoom(_)) => {
                    info!(
                        "[loop_recv] user {:?} joined room {:?}",
                        event.by_user_id, event.in_room_id
                    );
                }
                Some(event::Event::UserLeftTheRoom(_)) => {
                    info!(
                        "[loop_recv] user {:?} left room {:?}",
                        event.by_user_id, event.in_room_id
                    );
                }
            },
        };
    }
}

async fn paint(app: &mut ApplicationContext<'_>, drawing: Drawing) {
    let col = match drawing.color() {
        drawing::Color::White => color::WHITE,
        _ => color::BLACK,
    };
    let (xs, ys, ps, ws) = (drawing.xs, drawing.ys, drawing.pressures, drawing.widths);
    for i in 0..(xs.len() - 2) {
        if i != 0 {
            delay_for(Duration::from_millis(2)).await;
        }
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
    }
}

async fn send_drawing(client: &mut WhiteboardClient<Channel>, drawing: Drawing, ctx: &Ctx) {
    let event = Event {
        created_at: 0,
        by_user_id: "".into(),
        in_room_id: "".into(),
        event: Some(event::Event::Drawing(drawing)),
    };
    let mut req = Request::new(SendEventReq {
        event: Some(event),
        room_ids: vec![ctx.args.flag_room.to_owned()],
    });
    add_xuser(&mut req, ctx.user_id.to_owned());
    info!("REQ = {:?}", req);
    let rep = client
        .send_event(req)
        .await
        .map_err(|e| error!("!Send: {:?}", e));
    info!("REP = {:?}", rep);
}
