use docopt::Docopt;
use itertools::Itertools;
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
use libremarkable::ui_extensions::element::UIConstraintRefresh;
use libremarkable::ui_extensions::element::UIElement;
use libremarkable::ui_extensions::element::UIElementWrapper;
use log::{debug, error, info, warn};
use marauder::drawings;
use marauder::fonts::emsdelight_swash_caps;
use marauder::modes::draw::DrawMode;
use marauder::proto::whiteboard::whiteboard_client::WhiteboardClient;
use marauder::proto::whiteboard::{drawing, event};
use marauder::proto::whiteboard::{Drawing, Event};
use marauder::proto::whiteboard::{RecvEventsReq, SendEventReq};
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Mutex;
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

#[derive(Debug)]
struct Scribble {
    color: color,
    mult: u32,
    pos_x: f32,
    pos_y: f32,
    pressure: i32,
}

const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: 2 + 70,
    left: 0,
    height: 1900,
    width: 1404,
};

lazy_static! {
    static ref PEOPLE_COUNT: AtomicU32 = AtomicU32::new(0);
    static ref UNPRESS_OBSERVED: AtomicBool = AtomicBool::new(false);
    static ref WACOM_IN_RANGE: AtomicBool = AtomicBool::new(false);
    static ref WACOM_HISTORY: Mutex<VecDeque<(cgmath::Point2<f32>, i32)>> =
        Mutex::new(VecDeque::new());
    static ref SCRIBBLES: Mutex<Vec<Scribble>> = Mutex::new(Vec::new());
    static ref TX: Mutex<Option<std::sync::mpsc::Sender<Drawing>>> = Mutex::new(None);
    static ref FONT: HashMap<String, Vec<Vec<(f32, f32)>>> = emsdelight_swash_caps().unwrap();
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
                border_color: color::WHITE,
            },
            ..Default::default()
        },
    );

    app.draw_elements();

    let appref0 = app.upgrade_ref();
    spawn(async move {
        paint_mouldings(appref0).await;
    });

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
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            info!("[loop_recv] spawn-ed");
            loop_recv(appref, ch2, ctx2).await;
            info!("[loop_recv] terminated");
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
            app.clear(btn == gpio::PhysicalButton::MIDDLE);
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

    paint_glyph(app, &digit, (-15000., -150., 0.085), 3992, 3, color).await;
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
}

/////////////////////////////////////////
//https://stackoverflow.com/a/47869373/1418165
/// produces: [ linear_interpol(start, end, i/steps) | i <- 0..steps ]
/// (does NOT include "end")
///
/// linear_interpol(a, b, p) = (1 - p) * a + p * b
pub struct FloatIterator {
    current: u32,
    current_back: u32,
    steps: u32,
    start: f32,
    end: f32,
}

impl FloatIterator {
    pub fn new(start: f32, end: f32, steps: u32) -> Self {
        FloatIterator {
            current: 0,
            current_back: steps,
            steps,
            start,
            end,
        }
    }

    /// calculates number of steps from (end - start) / step
    pub fn new_with_step(start: f32, end: f32, step: f32) -> Self {
        let steps = ((end - start) / step).abs().round() as u32;
        Self::new(start, end, steps)
    }

    pub fn length(&self) -> u32 {
        self.current_back - self.current
    }

    fn at(&self, pos: u32) -> f32 {
        let f_pos = pos as f32 / self.steps as f32;
        (1. - f_pos) * self.start + f_pos * self.end
    }

    /// panics (in debug) when len doesn't fit in usize
    fn usize_len(&self) -> usize {
        let l = self.length();
        debug_assert!(l <= ::std::usize::MAX as u32);
        l as usize
    }
}

impl Iterator for FloatIterator {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.current_back {
            return None;
        }
        let result = self.at(self.current);
        self.current += 1;
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let l = self.usize_len();
        (l, Some(l))
    }

    fn count(self) -> usize {
        self.usize_len()
    }
}

impl DoubleEndedIterator for FloatIterator {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.current >= self.current_back {
            return None;
        }
        self.current_back -= 1;
        let result = self.at(self.current_back);
        Some(result)
    }
}

impl ExactSizeIterator for FloatIterator {
    fn len(&self) -> usize {
        self.usize_len()
    }
}

fn top_bar(c: drawing::Color) -> Drawing {
    let (start, end): (usize, usize) = (1, CANVAS_REGION.width.try_into().unwrap());
    let xs = dots(start as f32, end as f32, 8.0);
    let count = xs.len();
    Drawing {
        xs,
        ys: vec![70.444; count],
        pressures: vec![3992; count],
        widths: vec![2; count],
        color: c as i32,
    }
}

fn dots(start: f32, end: f32, steps: f32) -> Vec<f32> {
    FloatIterator::new_with_step(start, end, steps).collect()
}

// https://www.michaelfogleman.com/
//https://store.michaelfogleman.com/products/elementary-cellular-automata
//https://github.com/fogleman/ribbon
//https://github.com/fogleman/terrarium
//https://github.com/fogleman/Tiling
//https://en.wikipedia.org/wiki/List_of_Euclidean_uniform_tilings
/////////////https://github.com/fogleman/ln
//https://en.wikipedia.org/wiki/Turtle_graphics
//https://pbs.twimg.com/media/ErkHD2xXcAUPMtq?format=png&name=orig
//https://oeis.org/A088218

async fn paint_glyph(
    app: &mut ApplicationContext<'_>,
    glyph: &Vec<Vec<(f32, f32)>>,
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
            .into_iter()
            .tuple_windows()
            .map(|((xa, ya), (xb, yb))| Drawing {
                xs: vec![k * (xa - x0), (k * (xa - x0 + xb - x0)) / 2., k * (xb - x0)],
                ys: vec![k * (ya - y0), (k * (ya - y0 + yb - y0)) / 2., k * (yb - y0)],
                pressures: vec![p; 3],
                widths: vec![w; 3],
                color: c.into(),
            })
            .collect();
        paint_vec(app, drawing).await;
    }
}
