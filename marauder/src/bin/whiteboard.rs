use crc_any::CRC;
use docopt::Docopt;
use itertools::Itertools;
use lazy_static::lazy_static;
use libremarkable::appctx::ApplicationContext;
use libremarkable::framebuffer::cgmath;
use libremarkable::framebuffer::cgmath::EuclideanSpace;
use libremarkable::framebuffer::common::*;
use libremarkable::framebuffer::common::{DISPLAYHEIGHT, DISPLAYWIDTH};
use libremarkable::framebuffer::refresh::PartialRefreshMode;
use libremarkable::framebuffer::storage;
use libremarkable::framebuffer::FramebufferDraw;
use libremarkable::framebuffer::FramebufferIO;
use libremarkable::framebuffer::FramebufferRefresh;
use libremarkable::input::gpio;
use libremarkable::input::multitouch;
use libremarkable::input::wacom;
use libremarkable::ui_extensions::element::UIConstraintRefresh;
use libremarkable::ui_extensions::element::UIElement;
use libremarkable::ui_extensions::element::UIElementWrapper;
use log::{debug, error, info, warn};
use marauder::drawings;
use marauder::fonts;
use marauder::modes::draw::DrawMode;
use marauder::proto::hypercards::screen_sharing_client::ScreenSharingClient;
use marauder::proto::hypercards::whiteboard_client::WhiteboardClient;
use marauder::proto::hypercards::SendScreenReq;
use marauder::proto::hypercards::{drawing, event};
use marauder::proto::hypercards::{Drawing, Event};
use marauder::proto::hypercards::{RecvEventsReq, SendEventReq};
use qrcode_generator::QrCodeEcc;
use serde::Deserialize;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::Duration;
use tokio::spawn;
use tokio::task::spawn_blocking;
use tokio::time::delay_for;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tonic::Request;
use uuid::Uuid;

const USAGE: &str = "
reMarkable whiteboard HyperCard.

Usage:
  whiteboard [--room=<ROOM>] [--host=<HOST>] [--webhost=<WEBHOST>]
  whiteboard (-h | --help)
  whiteboard --version

Options:
  --host=<HOST>        gRPC server to connect to [default: http://fknwkdacd.com:10000].
  --room=<ROOM>        Room to join [default: living-room].
  --webhost=<WEBHOST>  Screen sharing HTTP server [default: http://fknwkdacd.com:18888/s].
  -h --help            Show this screen.
  --version            Show version.
";

#[derive(Debug, Deserialize, Clone)]
struct Args {
    flag_host: String,
    flag_room: String,
    flag_webhost: String,
    // TODO: here try user_id: String,
}

#[derive(Debug, Clone)]
struct Ctx {
    args: Args,
    user_id: String,
}

impl ::std::default::Default for Ctx {
    fn default() -> Self {
        Ctx {
            args: Args {
                flag_host: "unset".to_string(),
                flag_room: "unset".to_string(),
                flag_webhost: "unset".to_string(),
            },
            user_id: "anon".to_string(),
        }
    }
}

#[derive(Debug)]
struct Scribble {
    color: color,
    mult: u32,
    pos_x: f32,
    pos_y: f32,
    pressure: i32,
}

const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 2 + 70, // TODO: make the top bar ~140
    left: 0,
    height: DISPLAYHEIGHT as u32 - (2 + 70),
    width: DISPLAYWIDTH as u32,
};

lazy_static! {
    static ref PEOPLE_COUNT: AtomicU32 = AtomicU32::new(0);
    static ref UNPRESS_OBSERVED: AtomicBool = AtomicBool::new(false);
    static ref WACOM_IN_RANGE: AtomicBool = AtomicBool::new(false);
    static ref WACOM_HISTORY: Mutex<VecDeque<(cgmath::Point2<f32>, i32)>> =
        Mutex::new(VecDeque::new());
    static ref SCRIBBLES: Mutex<Vec<Scribble>> = Mutex::new(Vec::new());
    static ref TX: Mutex<Option<std::sync::mpsc::Sender<Drawing>>> = Mutex::new(None);
    static ref FONT: fonts::Font = fonts::emsdelight_swash_caps().unwrap();
    static ref NEEDS_SHARING: AtomicBool = AtomicBool::new(true);
    static ref CTX: RwLock<Ctx> = RwLock::new(Default::default());
}

const DRAWING_PACE: Duration = Duration::from_millis(2);
const INTER_DRAWING_PACE: Duration = Duration::from_millis(8);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    debug!("args = {:?}", args);

    let user_id = Uuid::new_v4().to_hyphenated().to_string();
    debug!("user_id = {:?}", user_id);
    // TODO: save settings under /opt/hypercards/users/<user_id>/...
    let mut wctx = CTX.write().unwrap();
    *wctx = Ctx { args, user_id };

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
                border_color: color::WHITE,
            },
            ..Default::default()
        },
    );

    app.draw_elements();

    let appref1 = app.upgrade_ref();
    spawn(async move {
        paint_mouldings(appref1).await;
    });

    let host = CTX.read().unwrap().args.flag_host.clone();
    info!("[main] connecting to {:?}...", host);
    let ch = Endpoint::from_shared(host).unwrap().connect().await?;

    let ch2 = ch.clone();
    let appref2 = app.upgrade_ref();
    info!("[loop_recv] spawn-ing");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_recv] spawn-ed");
            loop_recv(appref2, ch2).await;
            info!("[loop_recv] terminated");
        })
    });

    let ch3 = ch.clone();
    let appref3 = app.upgrade_ref();
    info!("[loop_screensharing] spawn-ing");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_screensharing] spawn-ed");
            loop_screensharing(appref3, ch3).await;
            info!("[loop_screensharing] terminated");
        })
    });

    info!("[TXer] spawn-ing");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[TXer] spawn-ed");
            let (tx, rx) = std::sync::mpsc::channel();
            //TODO: let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            {
                info!("[TXer] locking");
                let mut wtx = TX.lock().unwrap();
                *wtx = Some(tx.to_owned());
                info!("[TXer] unlocked");
            }
            let mut client = WhiteboardClient::new(ch);
            loop {
                let rcvd = rx.recv();
                debug!("[TXer] FWDing...");
                match rcvd {
                    Err(e) => error!("[TXer] failed to FWD: {:?}", e),
                    Ok(drawing) => {
                        send_drawing(&mut client, drawing).await;
                        NEEDS_SHARING.store(true, Ordering::Relaxed);
                    }
                }
            }
        })
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
                let mut scribbles = SCRIBBLES.lock().unwrap();
                scribbles.push(Scribble {
                    color: col,
                    mult,
                    pos_x: position.x,
                    pos_y: position.y,
                    pressure: pressure as i32,
                });
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
    let mut scribbles = SCRIBBLES.lock().unwrap();
    let len = scribbles.len();
    if len < 3 {
        return;
    }
    debug!("scribbles.len() = {:?}", len);

    let mut ws = Vec::<u32>::with_capacity(len);
    let mut xs = Vec::<f32>::with_capacity(len);
    let mut ys = Vec::<f32>::with_capacity(len);
    let mut ps = Vec::<i32>::with_capacity(len);
    for i in 0..len {
        let scribble = &scribbles[i];
        ws.push(scribble.mult);
        xs.push(scribble.pos_x);
        ys.push(scribble.pos_y);
        ps.push(scribble.pressure);
    }

    let col = match scribbles[0].color {
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
    scribbles.clear();
}

fn on_tch(_app: &mut ApplicationContext, _input: multitouch::MultitouchEvent) {
    debug!("[on_tch]");
}

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
            info!(">>> pressed right button");
        }
        gpio::PhysicalButton::LEFT => {
            info!(">>> pressed left button");
        }
        gpio::PhysicalButton::MIDDLE => {
            app.clear(true);
            app.draw_elements();

            let appref = app.upgrade_ref();
            spawn_blocking(move || {
                tokio::runtime::Handle::current().block_on(async move {
                    paint_mouldings(appref).await;
                })
            });
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

fn add_xuser<T>(req: &mut Request<T>, user_id: String) {
    let md = Request::metadata_mut(req);
    let key = "x-user";
    assert!(md.get(key).is_none());
    md.insert(key, user_id.parse().unwrap());
}

async fn loop_screensharing(app: &mut ApplicationContext<'_>, ch: Channel) {
    info!("[loop_screensharing] creating client");
    let mut client = ScreenSharingClient::new(ch);
    info!("[loop_screensharing] created client");

    let mut previous_checksum: u64 = 0;
    let (mut failsafe, wrapat) = (0, 10);
    loop {
        delay_for(Duration::from_millis(500)).await;

        failsafe += 1;
        let wrapped = failsafe == wrapat;
        if wrapped {
            failsafe = 0;
        }
        if !(wrapped || NEEDS_SHARING.load(Ordering::Relaxed)) {
            continue;
        }

        debug!("[loop_screensharing] dumping canvas");
        let framebuffer = app.get_framebuffer_ref();
        let roi = CANVAS_REGION;
        match framebuffer.dump_region(roi) {
            Err(err) => error!("[loop_screensharing] failed to dump framebuffer: {0}", err),
            Ok(buff) => {
                let mut crc32 = CRC::crc32();
                crc32.digest(&buff);
                let new_checksum = crc32.get_crc();
                if new_checksum == previous_checksum {
                    NEEDS_SHARING.store(false, Ordering::Relaxed);
                    continue;
                }
                previous_checksum = new_checksum;
                debug!("[loop_screensharing] compressing canvas");
                if let Some(img0) =
                    storage::rgbimage_from_u8_slice(roi.width, roi.height, buff.as_slice())
                {
                    let img = image::DynamicImage::ImageRgb8(img0);
                    let mut compressed = Vec::with_capacity(50_000);
                    match img.write_to(&mut compressed, image::ImageOutputFormat::PNG) {
                        Err(err) => error!("[loop_screensharing] failed to compress fb: {:?}", err),
                        Ok(()) => {
                            info!("[loop_screensharing] compressed!");
                            let bytes = compressed.len();
                            let mut req = Request::new(SendScreenReq {
                                room_id: CTX.read().unwrap().args.flag_room.clone(),
                                screen_png: compressed,
                            });
                            add_xuser(&mut req, CTX.read().unwrap().user_id.clone());
                            debug!("[loop_screensharing] sending canvas");
                            match client.send_screen(req).await {
                                Err(err) => error!("[loop_screensharing] !send: {:?}", err),
                                Ok(_) => {
                                    NEEDS_SHARING.store(false, Ordering::Relaxed);
                                    debug!("[loop_screensharing] sent {} bytes", bytes);
                                }
                            }
                        }
                    }
                }
            }
        };
    }
}

async fn loop_recv(app: &mut ApplicationContext<'_>, ch: Channel) {
    let mut req = Request::new(RecvEventsReq {
        room_id: CTX.read().unwrap().args.flag_room.clone(),
    });
    add_xuser(&mut req, CTX.read().unwrap().user_id.clone());

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
                    NEEDS_SHARING.store(true, Ordering::Relaxed);
                }
                Some(event::Event::UsersInTheRoom(c)) => {
                    let old = PEOPLE_COUNT.swap(c, Ordering::Relaxed);
                    repaint_people_counter(app, old, c).await;
                    info!("[loop_recv] room {:?} has {:?} users", event.in_room_id, c);
                }
                Some(event::Event::UserJoinedTheRoom(_)) => {
                    info!("[loop_recv] user {:?} joined room", event.by_user_id);
                    let c = PEOPLE_COUNT.fetch_add(1, Ordering::Relaxed);
                    repaint_people_counter(app, c, c + 1).await;
                }
                Some(event::Event::UserLeftTheRoom(_)) => {
                    info!("[loop_recv] user {:?} left room", event.by_user_id);
                    let c = PEOPLE_COUNT.fetch_sub(1, Ordering::Relaxed);
                    repaint_people_counter(app, c, c - 1).await;
                }
                // Streamer MAY send never revisions of proto messages
                #[allow(unreachable_patterns)]
                Some(other) => warn!("[loop_recv] unhandled msg {:?}", other),
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
    assert!(xs.len() >= 3);
    for i in 0..(xs.len() - 2) {
        if i != 0 {
            delay_for(DRAWING_PACE).await;
        }
        let points: Vec<(cgmath::Point2<f32>, i32, u32)> = vec![
            // start
            (cgmath::Point2 { x: xs[i], y: ys[i] }, ps[i], ws[i]),
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

async fn send_drawing(client: &mut WhiteboardClient<Channel>, drawing: Drawing) {
    let event = Event {
        created_at: 0,
        by_user_id: "".into(),
        in_room_id: "".into(),
        event: Some(event::Event::Drawing(drawing)),
    };
    let mut req = Request::new(SendEventReq {
        event: Some(event),
        room_ids: vec![CTX.read().unwrap().args.flag_room.clone()],
    });
    add_xuser(&mut req, CTX.read().unwrap().user_id.clone());
    info!("REQ = {:?}", req);
    let rep = client
        .send_event(req)
        .await
        .map_err(|e| error!("!Send: {:?}", e));
    info!("REP = {:?}", rep);
}

async fn paint_people_counter(app: &mut ApplicationContext<'_>, count: u32, color: drawing::Color) {
    let digit = match count {
        0 => FONT.get("0").unwrap(),
        1 => FONT.get("1").unwrap(),
        2 => FONT.get("2").unwrap(),
        3 => FONT.get("3").unwrap(),
        4 => FONT.get("4").unwrap(),
        5 => FONT.get("5").unwrap(),
        6 => FONT.get("6").unwrap(),
        7 => FONT.get("7").unwrap(),
        8 => FONT.get("8").unwrap(),
        9 => FONT.get("9").unwrap(),
        _ => {
            info!("drawing PEOPLE_COUNT of 9 even though it's at {:?}", count);
            FONT.get("9").unwrap()
        }
    };

    paint_glyph(app, digit, (-15000., -150., 0.085), 3992, 3, color).await;
}

async fn paint_vec(app: &mut ApplicationContext<'_>, xs: Vec<Drawing>) {
    for (i, x) in xs.into_iter().enumerate() {
        if i != 0 {
            delay_for(INTER_DRAWING_PACE).await;
        }
        paint(app, x).await;
    }
}

async fn repaint_people_counter(app: &mut ApplicationContext<'_>, o: u32, n: u32) {
    paint_people_counter(app, o, drawing::Color::White).await;
    paint_people_counter(app, n, drawing::Color::Black).await;
    paint(app, top_bar(drawing::Color::Black)).await;
}

async fn paint_mouldings(app: &mut ApplicationContext<'_>) {
    let c = drawing::Color::Black;
    debug!("[paint_mouldings] drawing UI...");
    paint_vec(app, drawings::title_whiteboard::f(c)).await;
    delay_for(INTER_DRAWING_PACE).await;
    let appref1 = app.upgrade_ref();
    spawn(async move {
        paint(appref1, top_bar(c)).await;
    });
    let appref2 = app.upgrade_ref();
    spawn(async move {
        paint_vec(appref2, drawings::top_left_help::f(c)).await;
    });
    let appref3 = app.upgrade_ref();
    spawn(async move {
        paint_vec(appref3, drawings::top_left_white_empty_square::f(c)).await;
    });
    let appref4 = app.upgrade_ref();
    spawn(async move {
        paint_vec(appref4, drawings::top_left_x3::f(c)).await;
    });
    let appref5 = app.upgrade_ref();
    spawn(async move {
        let count = PEOPLE_COUNT.load(Ordering::Relaxed);
        paint_people_counter(appref5, count, c).await;
    });
    let appref6 = app.upgrade_ref();
    spawn(async move {
        let webhost = CTX.read().unwrap().args.flag_webhost.clone();
        let url = webhost + "/" + &CTX.read().unwrap().args.flag_room + "/";
        debug!("[qrcode] generating");
        let qrcode: Vec<u8> = qrcode_generator::to_png_to_vec(url, QrCodeEcc::Low, 128).unwrap();
        debug!("[qrcode] loading");
        let img_rgb565 = image::load_from_memory(&qrcode).unwrap();
        let img_rgb = img_rgb565.to_rgb();
        let fb = appref6.get_framebuffer_ref();
        debug!("[qrcode] painting");
        debug!(
            "[qrcode] >>> {:?} {}x{}",
            CANVAS_REGION.top_left(),
            img_rgb.width(),
            img_rgb.height()
        ); ////////////////////////////////////////// [2021-07-20T15:11:11Z DEBUG whiteboard] [qrcode] >>> Point2 [0, 72] 128x128
        fb.draw_image(&img_rgb, CANVAS_REGION.top_left().cast().unwrap());
        fb.partial_refresh(
            &CANVAS_REGION,
            PartialRefreshMode::Wait,
            waveform_mode::WAVEFORM_MODE_GC16,
            display_temp::TEMP_USE_PAPYRUS,
            dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
            0,
            false,
        );
        debug!("[qrcode] done");
    });
}

fn top_bar(c: drawing::Color) -> Drawing {
    let max_x: u32 = CANVAS_REGION.width;
    let mut xs: Vec<f32> = Vec::with_capacity(max_x.try_into().unwrap());
    for i in 1..xs.capacity() {
        xs.push(i as f32);
    }
    let count = xs.len();
    Drawing {
        xs,
        ys: vec![CANVAS_REGION.top as f32 - 2.; count],
        pressures: vec![3992; count],
        widths: vec![2; count],
        color: c as i32,
    }
}

async fn paint_glyph(
    app: &mut ApplicationContext<'_>,
    glyph: &[Vec<(f32, f32)>],
    c0k: (f32, f32, f32),
    p: i32,
    w: u32,
    c: drawing::Color,
) {
    let (x0, y0, k) = c0k;
    for (i, path) in glyph.iter().enumerate() {
        if i != 0 {
            delay_for(INTER_DRAWING_PACE).await;
        }
        let drawing: Vec<Drawing> = path
            .iter()
            .tuple_windows()
            .map(|((xa, ya), (xb, yb))| {
                let xs = vec![k * (xa - x0), (k * (xa - x0 + xb - x0)) / 2., k * (xb - x0)];
                let ys = vec![k * (ya - y0), (k * (ya - y0 + yb - y0)) / 2., k * (yb - y0)];
                let points_count = xs.len();
                Drawing {
                    xs,
                    ys,
                    pressures: vec![p; points_count],
                    widths: vec![w; points_count],
                    color: c.into(),
                }
            })
            .collect();
        paint_vec(app, drawing).await;
    }
}
