use std::sync::mpsc; // TODO? let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
use std::{
    collections::VecDeque,
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, RwLock,
    },
    time::Duration,
};

use clap::Parser;
use crc_any::CRC;
use itertools::Itertools;
use libremarkable::{
    appctx,
    appctx::ApplicationContext,
    cgmath::Point2,
    framebuffer::{
        cgmath,
        cgmath::EuclideanSpace,
        common::{
            color, display_temp, dither_mode, mxcfb_rect, waveform_mode, DISPLAYHEIGHT,
            DISPLAYWIDTH, DRAWING_QUANT_BIT,
        },
        storage, FramebufferDraw, FramebufferIO, FramebufferRefresh, PartialRefreshMode,
    },
    image,
    input::{Finger, GPIOEvent, InputEvent, MultitouchEvent, PhysicalButton, WacomEvent, WacomPen},
    ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementWrapper},
};
use log::{debug, error, info, warn};
use marauder::fonts;
use once_cell::sync::Lazy;
use pb::proto::hypercards::{
    drawing::Color, event, screen_sharing_client::ScreenSharingClient,
    whiteboard_client::WhiteboardClient, Drawing, Event, RecvEventsReq, SendEventReq,
    SendScreenReq,
};
use qrcode_generator::QrCodeEcc;
use tokio::{spawn, task::spawn_blocking, time::sleep};
use tonic::{
    metadata::AsciiMetadataValue,
    transport::{Channel, Endpoint},
    Request,
};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[clap(name = "whiteboard", about = "reMarkable whiteboard HyperCard")]
struct Args {
    /// Room to join
    #[arg(long = "room", default_value = "living-room")]
    flag_room: String,

    /// Host to connect to
    #[arg(long = "host", default_value = "http://fknwkdacd.com:10000")]
    flag_host: String,

    /// Web host to send live feed to
    #[arg(long = "webhost", default_value = "http://fknwkdacd.com:18888/s")]
    flag_webhost: String,

    /// ID to identify as
    #[arg(skip)]
    user_id: String,
}

impl ::std::default::Default for Args {
    fn default() -> Self {
        Self {
            flag_host: "unset".to_owned(),
            flag_room: "unset".to_owned(),
            flag_webhost: "unset".to_owned(),
            user_id: "anon".to_owned(),
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

const TOOLBAR_BAR_WIDTH: u32 = 2;
const TOOLBAR_HEIGHT: u32 = 70 + TOOLBAR_BAR_WIDTH;
const TOOLBAR_REGION: mxcfb_rect =
    mxcfb_rect { top: 0, left: 0, height: TOOLBAR_HEIGHT, width: DISPLAYWIDTH as u32 };
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: TOOLBAR_HEIGHT,
    left: 0,
    height: DISPLAYHEIGHT as u32 - TOOLBAR_HEIGHT,
    width: DISPLAYWIDTH as u32,
};

type SomeRawImage = image::ImageBuffer<image::Rgb<u8>, Vec<u8>>;
type PosNpress = (cgmath::Point2<f32>, i32); // position and pressure

static PEOPLE_COUNT: Lazy<AtomicU32> = Lazy::new(|| AtomicU32::new(0));
static UNPRESS_OBSERVED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static WACOM_IN_RANGE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));
static WACOM_HISTORY: Lazy<Mutex<VecDeque<PosNpress>>> = Lazy::new(|| Mutex::new(VecDeque::new()));
static SCRIBBLES: Lazy<Mutex<Vec<Scribble>>> = Lazy::new(|| Mutex::new(Vec::new()));
static TX: Lazy<Mutex<Option<mpsc::Sender<Drawing>>>> = Lazy::new(|| Mutex::new(None));
static FONT: Lazy<fonts::Font> = Lazy::new(|| fonts::emsdelight_swash_caps().unwrap()); // TODO: const fn
static NEEDS_SHARING: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(true));
static ARGS: Lazy<RwLock<Args>> = Lazy::new(|| RwLock::new(Default::default()));
static QRCODE: Lazy<RwLock<Option<SomeRawImage>>> = Lazy::new(|| RwLock::new(None));
static CHER: Lazy<RwLock<Option<Channel>>> = Lazy::new(|| RwLock::new(None));

static PEN_BLACK: Lazy<AtomicBool> =
    Lazy::new(|| AtomicBool::new(matches!(black(true), color::BLACK)));

const DRAWING_PACE: Duration = Duration::from_millis(2);
const INTER_DRAWING_PACE: Duration = Duration::from_millis(8);

fn maybe_from_env(val: &mut String, var: &str) {
    if let Ok(newval) = std::env::var(var) {
        info!("using {var:?} from env: {newval:?}");
        *val = newval;
    }
}

fn black(x: bool) -> color {
    if x {
        color::BLACK
    } else {
        color::WHITE
    }
}

#[test]
fn color2bool() {
    assert_eq!(black(true), color::BLACK);
    assert_eq!(black(false), color::WHITE);
    assert!(matches!(black(true), color::BLACK));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut args = Args::parse();
    args.user_id = Uuid::new_v4().hyphenated().to_string();
    maybe_from_env(&mut args.flag_room, "WHITEBOARD_ROOM");
    maybe_from_env(&mut args.flag_host, "WHITEBOARD_HOST");
    maybe_from_env(&mut args.flag_webhost, "WHITEBOARD_WEBHOST");
    info!("args = {args:?}");
    // TODO: save settings under /opt/hypercards/users/<user_id>/...

    {
        let mut wargs = ARGS.write().unwrap();
        *wargs = args;
    }

    // TODO: check for updates when asked:
    // reqwest JSON API equivalent of https://github.com/fenollp/reMarkable-tools/releases
    // and select the highest semver these
    // DL + decompress + checksum + chmod + move + execve
    // unless version is current

    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();
    app.clear(true);

    app.add_element(
        "canvasRegion",
        UIElementWrapper {
            position: CANVAS_REGION.top_left().cast().unwrap() + cgmath::vec2(0, -2),
            refresh: UIConstraintRefresh::RefreshAndWait,
            inner: UIElement::Region {
                size: CANVAS_REGION.size().cast().unwrap() + cgmath::vec2(1, 3),
                border_px: 0,
                border_color: color::BLACK,
            },
            ..Default::default()
        },
    );

    app.draw_elements();

    let appref1 = app.upgrade_ref();
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            paint_mouldings(appref1).await;
        });
    });

    {
        let host = ARGS.read().unwrap().flag_host.clone();
        info!("[main] using gRPC host: {host:?}");
        let uaprexix = "https://github.com/fenollp/reMarkable-tools/releases/tag/v".to_owned();
        let endpoint = Endpoint::from_shared(host)
            .unwrap()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(4))
            .user_agent(uaprexix + env!("CARGO_PKG_VERSION"))
            .unwrap();
        let mut wcher = CHER.write().unwrap();
        *wcher = Some(endpoint.connect_lazy());
    }

    let appref2 = app.upgrade_ref();
    info!("[main] spawn-ing loop_recv");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_recv] spawn-ed");
            loop {
                if let Some(ch) = CHER.read().map(|ch| ch.clone()).unwrap() {
                    loop_recv(appref2, ch).await;
                    break;
                }
            }
            info!("[loop_recv] terminated");
        })
    });

    let appref3 = app.upgrade_ref();
    info!("[main] spawn-ing loop_screensharing");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_screensharing] spawn-ed");
            loop {
                if let Some(ch) = CHER.read().map(|ch| ch.clone()).unwrap() {
                    loop_screensharing(appref3, ch).await;
                    break;
                }
            }
            info!("[loop_screensharing] terminated");
        })
    });

    info!("[main] spawn-ing TXer");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[TXer] spawn-ed");
            let (tx, rx) = mpsc::channel();
            {
                info!("[TXer] locking");
                let mut wtx = TX.lock().unwrap();
                *wtx = Some(tx.to_owned());
                info!("[TXer] unlocked");
            }
            loop {
                if let Some(ch) = CHER.read().map(|ch| ch.clone()).unwrap() {
                    let mut client = WhiteboardClient::new(ch);
                    loop {
                        let rcvd = rx.recv();
                        debug!("[TXer] FWDing...");
                        match rcvd {
                            Err(e) => error!("[TXer] failed to FWD: {e:?}"),
                            Ok(drawing) => {
                                send_drawing(&mut client, drawing).await;
                                NEEDS_SHARING.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
        })
    });

    info!("[main] spawn-ing qrcoder");
    spawn(async move {
        info!("[qrcoder] spawn-ed");
        let webhost = ARGS.read().unwrap().flag_webhost.clone();
        let url = webhost + "/" + &ARGS.read().unwrap().flag_room + "/";
        debug!("[qrcoder] generating");
        let qrcode: Vec<u8> = qrcode_generator::to_png_to_vec(url, QrCodeEcc::Low, 64).unwrap();
        debug!("[qrcoder] loading");
        let img_rgb565 = image::load_from_memory(&qrcode).unwrap();
        let img_rgb = img_rgb565.to_rgb8();
        let mut wqrcode = QRCODE.write().unwrap();
        *wqrcode = Some(img_rgb);
        info!("[qrcoder] done");
    });

    info!("Init complete. Beginning event dispatch...");
    app.start_event_loop(true, true, true, |ctx, evt| match evt {
        InputEvent::WacomEvent { event } => on_pen(ctx, event),
        InputEvent::MultitouchEvent { event } => on_tch(ctx, event),
        InputEvent::GPIO { event } => on_btn(ctx, event),
        InputEvent::Unknown {} => {}
    });

    Ok(())
}

fn on_pen(app: &mut ApplicationContext, input: WacomEvent) {
    match input {
        WacomEvent::Draw { position, pressure, tilt: _ } => {
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

            let col = black(PEN_BLACK.load(Ordering::Relaxed));
            let mult = if col == color::WHITE { 64 } else { 2 };

            {
                let mut scribbles = SCRIBBLES.lock().unwrap();
                scribbles.push(Scribble {
                    color: col,
                    mult,
                    pos_x: position.x,
                    pos_y: position.y,
                    pressure: i32::from(pressure),
                });
            }

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
                WacomPen::ToolPen | WacomPen::ToolRubber => {
                    let in_range = state; // Whether the pen is in range
                    WACOM_IN_RANGE.store(in_range, Ordering::Relaxed);
                    let is_white = matches!(pen, WacomPen::ToolRubber);
                    info!("changing color to {:?}", black(!is_white));
                    PEN_BLACK.store(!is_white, Ordering::Relaxed);
                }
                WacomPen::Touch | WacomPen::Stylus | WacomPen::Stylus2 => {
                    // Whether the pen is actually making contact
                    let making_contact = state;
                    if !making_contact {
                        let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                        wacom_stack.clear();
                        maybe_send_drawing();
                    }
                }
            }
        }
        WacomEvent::Hover { position: _, distance, tilt: _ } => {
            // If the pen is hovering, don't record its coordinates as the origin of the next line
            if distance > 1 {
                let mut wacom_stack = WACOM_HISTORY.lock().unwrap();
                wacom_stack.clear();
                maybe_send_drawing();

                UNPRESS_OBSERVED.store(true, Ordering::Relaxed);
            }
        }
        WacomEvent::Unknown => info!("got WacomEvent::Unknown"),
    };
}

fn maybe_send_drawing() {
    let mut scribbles = SCRIBBLES.lock().unwrap();
    let len = scribbles.len();
    if len < 3 {
        return;
    }
    debug!("scribbles.len() = {len:?}");

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
        color::WHITE => Color::White,
        _ => Color::Black,
    };

    debug!("locking TX");
    if let Some(ref tx) = *TX.lock().unwrap() {
        let drawing = Drawing { xs, ys, pressures: ps, widths: ws, color: col as i32 };
        tx.send(drawing).unwrap();
        debug!("unlocked TX");
    }
    scribbles.clear();
}

fn on_tch(_app: &mut ApplicationContext, input: MultitouchEvent) {
    match input {
        MultitouchEvent::Release { finger: Finger { pos: Point2 { x, y }, .. }, .. } => {
            info!("[on_tch] finger on zone x:{x} y:{y}")
        }
        _ => debug!("[on_tch] {input:?}"),
    }
}

fn on_btn(app: &mut ApplicationContext, input: GPIOEvent) {
    let (btn, pressed) = match input {
        GPIOEvent::Press { button } => (button, true),
        GPIOEvent::Unpress { button } => (button, false),
        GPIOEvent::Unknown => return,
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
        PhysicalButton::RIGHT => {
            info!(">>> pressed right button");
        }
        PhysicalButton::LEFT => {
            info!(">>> pressed left button");
        }
        PhysicalButton::MIDDLE => {
            app.clear(true);
            app.draw_elements();

            let appref = app.upgrade_ref();
            spawn_blocking(move || {
                tokio::runtime::Handle::current().block_on(async move {
                    paint_mouldings(appref).await;
                })
            });
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

fn add_xuser<T>(req: &mut Request<T>, user_id: String) {
    let md = Request::metadata_mut(req);
    let key = "x-user";
    assert!(md.get(key).is_none());
    let user_id: AsciiMetadataValue = user_id.parse().unwrap();
    md.insert(key, user_id);
}

async fn loop_screensharing(app: &mut ApplicationContext<'_>, ch: Channel) {
    info!("[loop_screensharing] creating client");
    let mut client = ScreenSharingClient::new(ch);
    info!("[loop_screensharing] created client");

    let mut previous_checksum: u64 = 0;
    let (mut failsafe, wrapat) = (0, 10);
    loop {
        sleep(Duration::from_millis(500)).await;

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
            // https://github.com/canselcik/libremarkable/pull/96
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
                    match img.write_to(&mut compressed, image::ImageOutputFormat::Png) {
                        Err(err) => error!("[loop_screensharing] failed to compress fb: {err:?}"),
                        Ok(()) => {
                            info!("[loop_screensharing] compressed!");
                            let bytes = compressed.len();
                            let mut req = Request::new(SendScreenReq {
                                room_id: ARGS.read().unwrap().flag_room.clone(),
                                screen_png: compressed,
                            });
                            add_xuser(&mut req, ARGS.read().unwrap().user_id.clone());
                            debug!("[loop_screensharing] sending canvas");
                            match client.send_screen(req).await {
                                Err(err) => error!("[loop_screensharing] !send: {err:?}"),
                                Ok(_) => {
                                    NEEDS_SHARING.store(false, Ordering::Relaxed);
                                    debug!("[loop_screensharing] sent {bytes} bytes");
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
    let req = RecvEventsReq { room_id: ARGS.read().unwrap().flag_room.clone() };
    let user_id = ARGS.read().unwrap().user_id.clone();

    let ms = 100;
    info!("[loop_recv] creating stream");
    let mut client = WhiteboardClient::new(ch);
    let mut stream = {
        let mut delays = (0..).map(|n| 2u64.pow(n)).take_while(|n| *n < ms);

        loop {
            let mut req = Request::new(req.clone());
            add_xuser(&mut req, user_id.clone());

            match client.recv_events(req).await {
                Ok(r) => {
                    info!("[loop_recv] Connection established!");
                    break r.into_inner();
                }
                Err(e) => {
                    let pause = Duration::from_millis(delays.next().unwrap_or(ms));
                    warn!("[loop_recv] Couldn't connect, next attempt in {pause:?}: {e}");
                    sleep(pause).await
                }
            }
        }
    };
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
                    info!("[loop_recv] room {:?} has {c:?} users", event.in_room_id);
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
                // Streamer MAY send newer revisions of proto messages
                #[allow(unreachable_patterns)]
                Some(other) => warn!("[loop_recv] unhandled msg {other:?}"),
            },
        };
    }
}

async fn paint(app: &mut ApplicationContext<'_>, drawing: Drawing) {
    let col = match drawing.color() {
        Color::White => color::WHITE,
        _ => color::BLACK,
    };
    let (xs, ys, ps, ws) = (drawing.xs, drawing.ys, drawing.pressures, drawing.widths);
    assert!(xs.len() >= 3);
    for i in 0..(xs.len() - 2) {
        if i != 0 {
            sleep(DRAWING_PACE).await;
        }
        let points: Vec<(cgmath::Point2<f32>, i32, u32)> = vec![
            // start
            (cgmath::Point2 { x: xs[i], y: ys[i] }, ps[i], ws[i]),
            // ctrl
            (cgmath::Point2 { x: xs[i + 1], y: ys[i + 1] }, ps[i + 1], ws[i + 1]),
            // end
            (cgmath::Point2 { x: xs[i + 2], y: ys[i + 2] }, ps[i + 2], ws[i + 2]),
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
        room_ids: vec![ARGS.read().unwrap().flag_room.clone()],
    });
    add_xuser(&mut req, ARGS.read().unwrap().user_id.clone().parse().unwrap());
    info!("REQ = {req:?}");
    let rep = client.send_event(req).await.map_err(|e| error!("!Send: {e:?}"));
    info!("REP = {rep:?}");
}

async fn paint_people_counter(app: &mut ApplicationContext<'_>, count: u32, color: Color) {
    let digit = match count {
        0 => FONT.get("0"),
        1 => FONT.get("1"),
        2 => FONT.get("2"),
        3 => FONT.get("3"),
        4 => FONT.get("4"),
        5 => FONT.get("5"),
        6 => FONT.get("6"),
        7 => FONT.get("7"),
        8 => FONT.get("8"),
        9 => FONT.get("9"),
        _ => {
            info!("drawing PEOPLE_COUNT of 9 even though it's at {count:?}");
            FONT.get("9")
        }
    }
    .unwrap();

    let at = (-15000., -150. * 5., 0.085);
    paint_glyph(app, digit, at, 3992, 3, color).await;
}

async fn paint_vec(app: &mut ApplicationContext<'_>, xs: Vec<Drawing>) {
    for (i, x) in xs.into_iter().enumerate() {
        if i != 0 {
            sleep(INTER_DRAWING_PACE).await;
        }
        paint(app, x).await;
    }
}

async fn repaint_people_counter(app: &mut ApplicationContext<'_>, o: u32, n: u32) {
    paint_people_counter(app, o, Color::White).await;
    paint_people_counter(app, n, Color::Black).await;
    paint(app, top_bar(Color::Black)).await;
}

async fn paint_mouldings(app: &mut ApplicationContext<'_>) {
    let c = Color::Black;
    debug!("[paint_mouldings] drawing UI...");

    let appref0 = app.upgrade_ref();
    spawn(async move {
        let mut parts = drawings::title_whiteboard::f(c);
        for part in &mut parts {
            for w in &mut part.widths {
                *w /= 2;
            }
        }
        paint_vec(appref0, parts).await;
    });

    let appref6 = app.upgrade_ref();
    spawn(async move {
        loop {
            match QRCODE.read().unwrap().as_ref() {
                None => (),
                Some(qrcode) => {
                    debug!("[qrcode] painting");
                    let fb = appref6.get_framebuffer_ref();
                    let region = mxcfb_rect {
                        top: 4,
                        left: TOOLBAR_REGION.width - (4 + qrcode.width()),
                        height: qrcode.height(),
                        width: qrcode.width(),
                    };
                    fb.draw_image(qrcode, region.top_left().cast().unwrap());
                    fb.partial_refresh(
                        &region,
                        PartialRefreshMode::Async,
                        waveform_mode::WAVEFORM_MODE_GC16,
                        display_temp::TEMP_USE_PAPYRUS,
                        dither_mode::EPDC_FLAG_USE_DITHERING_PASSTHROUGH,
                        0,
                        false,
                    );
                    debug!("[qrcode] done");
                    break;
                }
            };
        }
    });

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
}

fn top_bar(c: Color) -> Drawing {
    let max_x: u32 = CANVAS_REGION.width;
    let mut xs: Vec<f32> = Vec::with_capacity(max_x.try_into().unwrap());
    for i in 1..xs.capacity() {
        xs.push(i as f32);
    }
    let count = xs.len();
    Drawing {
        xs,
        ys: vec![TOOLBAR_HEIGHT as f32 - TOOLBAR_BAR_WIDTH as f32; count],
        pressures: vec![3992; count],
        widths: vec![TOOLBAR_BAR_WIDTH; count],
        color: c as i32,
    }
}

async fn paint_glyph(
    app: &mut ApplicationContext<'_>,
    glyph: &[Vec<(f32, f32)>],
    c0k: (f32, f32, f32),
    p: i32,
    w: u32,
    c: Color,
) {
    let (x0, y0, k) = c0k;
    for (i, path) in glyph.iter().enumerate() {
        if i != 0 {
            sleep(INTER_DRAWING_PACE).await;
        }
        let drawing: Vec<Drawing> = path
            .iter()
            .tuple_windows()
            .map(|((xa, ya), (xb, yb))| {
                let (xa, ya) = (xa, -ya);
                let (xb, yb) = (xb, -yb);
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
