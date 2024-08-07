use std::{
    collections::VecDeque,
    process::{self, Command},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver},
        LazyLock, Mutex, OnceLock, RwLock,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use crc_any::CRC;
use itertools::Itertools;
use libremarkable::{
    appctx::ApplicationContext,
    cgmath::{vec2, EuclideanSpace, Point2},
    framebuffer::{
        common::{
            color, display_temp, dither_mode, mxcfb_rect, waveform_mode, DISPLAYHEIGHT,
            DISPLAYWIDTH, DRAWING_QUANT_BIT,
        },
        storage, FramebufferDraw, FramebufferIO, FramebufferRefresh, PartialRefreshMode,
    },
    image,
    input::{GPIOEvent, InputEvent, MultitouchEvent, WacomEvent, WacomPen},
    ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementWrapper},
};
use log::{debug, error, info, warn};
use marauder::{
    buttons::Button,
    fonts::{self, Font},
};
use pb::proto::hypercards::{
    drawing::Color, event, screen_sharing_client::ScreenSharingClient,
    whiteboard_client::WhiteboardClient, Drawing, Event, RecvEventsReq, SendEventReq,
    SendScreenReq,
};
use qrcode_generator::QrCodeEcc;
use tokio::{
    spawn,
    task::spawn_blocking,
    time::{interval, sleep},
};
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
    #[arg(long, env = "WHITEBOARD_ROOM", default_value = "living-room")]
    room: String,

    /// Host to connect to
    #[arg(long, env = "WHITEBOARD_HOST", default_value = "http://fknwkdacd.com:10000")]
    host: String,

    /// Web host to send live feed to
    #[arg(long, env = "WHITEBOARD_WEBHOST", default_value = "http://fknwkdacd.com:18888/s")]
    webhost: String,

    /// ID to identify as
    #[arg(skip)]
    user_id: String,
}

#[derive(Debug)]
struct Scribble {
    color: color,
    mult: u32,
    pos_x: f32,
    pos_y: f32,
    pressure: i32,
}

const REPO: &str = env!("CARGO_PKG_REPOSITORY");
const VSN: &str = env!("CARGO_PKG_VERSION");

const TOOLBAR_BAR_WIDTH: u32 = 2;
const TOOLBAR_HEIGHT: u32 = 70 + TOOLBAR_BAR_WIDTH;
const TOOLBAR_REGION: mxcfb_rect =
    mxcfb_rect { top: 0, left: 0, height: TOOLBAR_HEIGHT, width: DISPLAYWIDTH as u32 };
// (0,0) --x-> (x=1404,0)
// |
// y
// |
// v
// (0,1872)
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: TOOLBAR_HEIGHT,
    left: 0,
    height: DISPLAYHEIGHT as u32 - TOOLBAR_HEIGHT,
    width: DISPLAYWIDTH as u32,
};

type SomeRawImage = image::ImageBuffer<image::Rgb<u8>, Vec<u8>>;
type PosNpress = (Point2<f32>, i32); // position and pressure

static PEOPLE_COUNT: LazyLock<AtomicU32> = LazyLock::new(Default::default);
static WACOM_IN_RANGE: LazyLock<AtomicBool> = LazyLock::new(Default::default);
static WACOM_HISTORY: LazyLock<Mutex<VecDeque<PosNpress>>> = LazyLock::new(Default::default);
static SCRIBBLES: LazyLock<Mutex<Vec<Scribble>>> = LazyLock::new(Default::default);
static TX: LazyLock<Mutex<Option<mpsc::Sender<Drawing>>>> = LazyLock::new(Default::default);
static FONT: LazyLock<Font> = LazyLock::new(|| fonts::emsdelight_swash_caps().unwrap());
static NEEDS_SHARING: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(true));
static QRCODE: LazyLock<RwLock<Option<SomeRawImage>>> = LazyLock::new(Default::default);

static ARGS: OnceLock<Args> = OnceLock::new();

static PEN_BLACK: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(true));

static BTN_ERASE: LazyLock<Button> = LazyLock::new(|| Button::new(1, "erase"));
static BTN_TIMES3: LazyLock<Button> = LazyLock::new(|| Button::new(2, "times3"));

const DRAWING_PACE: Duration = Duration::from_millis(2);
const INTER_DRAWING_PACE: Duration = Duration::from_millis(8);

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
async fn main() -> Result<()> {
    env_logger::init();

    let mut args = Args::parse();
    args.user_id = Uuid::new_v4().hyphenated().to_string();
    info!("args = {args:?}");
    // TODO: save settings under /opt/hypercards/users/<user_id>/...
    let args = ARGS.get_or_init(|| args);

    // TODO: check for updates when asked:
    // reqwest JSON API equivalent of https://github.com/fenollp/reMarkable-tools/releases
    // and select the highest semver these
    // DL + decompress + checksum + chmod + move + execve
    // unless version is current

    let mut app: ApplicationContext<'_> = ApplicationContext::default();
    app.clear(true);

    app.add_element(
        "canvasRegion",
        UIElementWrapper {
            position: CANVAS_REGION.top_left().cast().unwrap() + vec2(0, -2),
            refresh: UIConstraintRefresh::RefreshAndWait,
            inner: UIElement::Region {
                size: CANVAS_REGION.size().cast().unwrap() + vec2(1, 3),
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

    let ch = {
        let host = args.host.clone();
        info!("[main] using gRPC host: {host:?}");
        Endpoint::from_shared(host)
            .unwrap()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(4))
            .user_agent(format!("{REPO}/releases/tag/v{VSN}"))
            .unwrap()
            .connect_lazy()
    };

    let ch2 = ch.clone();
    let appref2 = app.upgrade_ref();
    info!("[main] spawn-ing loop_recv");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_recv] spawn-ed");
            loop {
                if let Err(e) = loop_recv(appref2, ch2.clone()).await {
                    error!("[loop_recv] respawning due to: {e}");
                }
            }
        })
    });

    let ch3 = ch.clone();
    let appref3 = app.upgrade_ref();
    info!("[main] spawn-ing loop_screensharing");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_screensharing] spawn-ed");
            loop {
                if let Err(e) = loop_screensharing(appref3, ch3.clone()).await {
                    error!("[loop_screensharing] respawning due to: {e}");
                }
            }
        })
    });

    let ch4 = ch.clone();
    let (tx, rx) = mpsc::channel();
    *TX.lock().unwrap() = Some(tx.to_owned());
    info!("[main] spawn-ing loop_fwd");
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_fwd] spawn-ed");
            if let Err(e) = loop_fwd(rx, ch4.clone()).await {
                error!("[loop_fwd] terminating due to: {e}");
                // TODO: find a way to survive: re-set tx?
            }
        })
    });

    info!("[main] spawn-ing qrcoder");
    spawn(async move {
        info!("[qrcoder] spawn-ed");
        let url = format!("{}/{}/", args.webhost, args.room);
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
                wacom_stack.clear();
                maybe_send_drawing();
                return;
            }

            let from_pen = PEN_BLACK.load(Ordering::Relaxed);
            let from_btn = BTN_ERASE.is_pressed();
            let col = black(from_pen && !from_btn);
            let mult = if col == color::WHITE { 50 } else { 2 };
            let mult = mult * if BTN_TIMES3.is_pressed() { 3 } else { 1 };

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

fn on_tch(_: &mut ApplicationContext, input: MultitouchEvent) {
    BTN_ERASE.process_event(input);
    BTN_TIMES3.process_event(input);
}

fn on_btn(_: &mut ApplicationContext, input: GPIOEvent) {
    info!("[on_btn] input = {input:?}");

    if let GPIOEvent::Press { .. } = input {
        warn!("[on_btn] about to shut down & switch back to xochitl");
        Command::new("systemctl").arg("start").arg("xochitl").spawn().unwrap();
        process::exit(0);
    }
}

fn add_xuser<T>(req: &mut Request<T>, user_id: &str) -> Result<()> {
    let md = Request::metadata_mut(req);
    let key = "x-user";
    assert!(md.get(key).is_none());
    let user_id: AsciiMetadataValue = user_id.parse()?;
    md.insert(key, user_id);
    Ok(())
}

async fn loop_screensharing(app: &mut ApplicationContext<'_>, ch: Channel) -> Result<()> {
    let mut client = ScreenSharingClient::new(ch);

    let mut counter = 0;
    let mut previous_checksum: u64 = 0;
    let mut ticker = interval(Duration::from_millis(100));
    loop {
        ticker.tick().await;

        let wakeup = counter % 100 == 0; // Every 10s
        if wakeup {
            counter = 0;
        }
        counter += 1;
        if !(wakeup || NEEDS_SHARING.load(Ordering::Relaxed)) {
            continue;
        }

        debug!("[loop_screensharing] dumping canvas");
        let framebuffer = app.get_framebuffer_ref();
        let roi = CANVAS_REGION;
        // https://github.com/canselcik/libremarkable/pull/96
        let buff = framebuffer
            .dump_region(roi)
            .map_err(|e| anyhow!("[loop_screensharing] failed to dump framebuffer: {e}"))?;

        let mut crc32 = CRC::crc32();
        crc32.digest(&buff);
        let new_checksum = crc32.get_crc();
        if new_checksum == previous_checksum {
            NEEDS_SHARING.store(false, Ordering::Relaxed);
            continue;
        }
        previous_checksum = new_checksum;

        debug!("[loop_screensharing] compressing canvas");
        let Some(img) = storage::rgbimage_from_u8_slice(roi.width, roi.height, buff.as_slice())
        else {
            bail!("[loop_screensharing] Error compressing with rgbimage_from_u8_slice")
        };
        let img = image::DynamicImage::ImageRgb8(img);

        let mut compressed = Vec::with_capacity(50_000);
        img.write_to(&mut compressed, image::ImageOutputFormat::Png)
            .map_err(|e| anyhow!("[loop_screensharing] failed to compress fb: {e:?}"))?;
        info!("[loop_screensharing] compressed!");

        let Args { room, user_id, .. } = ARGS.get().expect("set on startup");
        let bytes = compressed.len();
        let mut req = Request::new(SendScreenReq { room_id: room.clone(), screen_png: compressed });
        add_xuser(&mut req, user_id)?;

        debug!("[loop_screensharing] sending canvas");
        client.send_screen(req).await.map_err(|e| anyhow!("[loop_screensharing] !send: {e:?}"))?;
        NEEDS_SHARING.store(false, Ordering::Relaxed);
        debug!("[loop_screensharing] sent {bytes} bytes");
    }
}

async fn loop_fwd(rx: Receiver<Drawing>, ch: Channel) -> Result<()> {
    let mut client = WhiteboardClient::new(ch);
    let Args { room, user_id, .. } = ARGS.get().expect("set on startup");
    loop {
        let rcvd = rx.recv();
        debug!("[loop_fwd] FWDing...");
        match rcvd {
            Err(e) => error!("[loop_fwd] failed to FWD: {e:?}"),
            Ok(drawing) => {
                send_drawing(&mut client, room, user_id, drawing).await?;
                NEEDS_SHARING.store(true, Ordering::Relaxed);
            }
        }
    }
}

async fn loop_recv(app: &mut ApplicationContext<'_>, ch: Channel) -> Result<()> {
    let Args { room, user_id, .. } = ARGS.get().expect("set on startup");
    let req = RecvEventsReq { room_id: room.clone() };

    let ms = 100;
    info!("[loop_recv] creating stream");
    let mut client = WhiteboardClient::new(ch.clone());
    let mut stream = {
        let mut delays = (0..).map(|n| 2u64.pow(n)).take_while(|n| *n < ms);

        loop {
            let mut req = Request::new(req.clone());
            add_xuser(&mut req, user_id)?;

            match client.recv_events(req).await {
                Ok(r) => {
                    info!("[loop_recv] connection established!");
                    break r.into_inner();
                }
                Err(e) => {
                    let pause = Duration::from_millis(delays.next().unwrap_or(ms));
                    warn!("[loop_recv] couldn't connect, next attempt in {pause:?}: {e}");
                    sleep(pause).await
                }
            }
        }
    };
    info!("[loop_recv] receiving...");

    loop {
        match stream.message().await {
            Err(e) => error!("[loop_recv] sender status: {e}"),
            Ok(None) => bail!("[loop_recv] connection dropped!"),
            Ok(Some(event)) => match event.event {
                None => error!("[loop_recv] empty event in message"),
                Some(event::Event::Drawing(drawing)) => {
                    let len = drawing.xs.len();
                    if len < 3 {
                        continue;
                    }
                    debug!("[loop_recv] drawing {len:?} points");
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
        let points: Vec<(Point2<f32>, i32, u32)> = vec![
            // start
            (Point2 { x: xs[i], y: ys[i] }, ps[i], ws[i]),
            // ctrl
            (Point2 { x: xs[i + 1], y: ys[i + 1] }, ps[i + 1], ws[i + 1]),
            // end
            (Point2 { x: xs[i + 2], y: ys[i + 2] }, ps[i + 2], ws[i + 2]),
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

async fn send_drawing(
    client: &mut WhiteboardClient<Channel>,
    room: &str,
    user_id: &str,
    drawing: Drawing,
) -> Result<()> {
    let event = Event {
        created_at: 0,
        by_user_id: "".into(),
        in_room_id: "".into(),
        event: Some(event::Event::Drawing(drawing)),
    };
    let mut req =
        Request::new(SendEventReq { event: Some(event), room_ids: vec![room.to_owned()] });
    add_xuser(&mut req, user_id)?;
    info!("REQ = {req:?}");
    if let Err(e) = client.send_event(req).await {
        bail!("[send_drawing] failure: {e}")
    }
    Ok(())
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
            sleep(Duration::from_millis(50)).await;
            if let Some(qrcode) = QRCODE.read().unwrap().as_ref() {
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
                info!("[qrcode] done");
                break;
            }
        }
    });

    let appref1 = app.upgrade_ref();
    spawn(async move {
        paint(appref1, top_bar(c)).await;
    });
    // let appref2 = app.upgrade_ref();
    // spawn(async move {
    //     paint_vec(appref2, drawings::top_left_help::f(c)).await;
    // });
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
