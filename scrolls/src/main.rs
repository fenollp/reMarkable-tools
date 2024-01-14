use std::{collections::HashSet, env, process::Command, time::Duration};

use anyhow::Result;
use function_name::named;
use libremarkable::{
    appctx,
    appctx::ApplicationContext,
    framebuffer::{
        cgmath,
        cgmath::EuclideanSpace,
        common::{
            color, display_temp, dither_mode, mxcfb_rect, waveform_mode, DISPLAYHEIGHT,
            DISPLAYWIDTH, DRAWING_QUANT_BIT,
        },
        FramebufferDraw, FramebufferRefresh, PartialRefreshMode,
    },
    input::{GPIOEvent, InputEvent, MultitouchEvent, WacomEvent},
    ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementWrapper},
};
use log::{debug, error, info};
use marauder::{
    drawings,
    proto::hypercards::{drawing, Drawing},
};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::Deserialize;
use tokio::{task::spawn_blocking, time::sleep};

const TOOLBAR_BAR_WIDTH: u32 = 2;
const TOOLBAR_HEIGHT: u32 = 70 + TOOLBAR_BAR_WIDTH;
const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
    top: TOOLBAR_HEIGHT,
    left: 0,
    height: DISPLAYHEIGHT as u32 - TOOLBAR_HEIGHT,
    width: DISPLAYWIDTH as u32,
};

const DRAWING_PACE: Duration = Duration::from_millis(2);
const INTER_DRAWING_PACE: Duration = Duration::from_millis(8);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

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

    let appref2 = app.upgrade_ref();
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            if let Err(e) = paint_scrolls(appref2).await {
                error!(target:"paint_scrolls", "Error: {e}");
            }
            Command::new("systemctl").arg("start").arg("xochitl").spawn().unwrap();
            std::process::exit(0);
        });
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

fn on_pen(_app: &mut ApplicationContext, input: WacomEvent) {
    debug!("[on_pen] {input:?}");
}

fn on_tch(_app: &mut ApplicationContext, input: MultitouchEvent) {
    debug!("[on_tch] {input:?}");
}

fn on_btn(_app: &mut ApplicationContext, input: GPIOEvent) {
    debug!("[on_btn] {input:?}");
}

async fn paint(app: &mut ApplicationContext<'_>, drawing: &Drawing) {
    let col = match drawing.color() {
        drawing::Color::White => color::WHITE,
        _ => color::BLACK,
    };
    assert!(drawing.xs.len() >= 3);
    for i in 0..(drawing.xs.len() - 2) {
        if i != 0 {
            sleep(DRAWING_PACE).await;
        }
        let points: Vec<(cgmath::Point2<f32>, i32, u32)> = vec![
            // start
            (
                cgmath::Point2 { x: drawing.xs[i], y: drawing.ys[i] },
                drawing.pressures[i],
                drawing.widths[i],
            ),
            // ctrl
            (
                cgmath::Point2 { x: drawing.xs[i + 1], y: drawing.ys[i + 1] },
                drawing.pressures[i + 1],
                drawing.widths[i + 1],
            ),
            // end
            (
                cgmath::Point2 { x: drawing.xs[i + 2], y: drawing.ys[i + 2] },
                drawing.pressures[i + 2],
                drawing.widths[i + 2],
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

async fn paint_vec(app: &mut ApplicationContext<'_>, xs: &[Drawing]) {
    for (i, x) in xs.iter().enumerate() {
        if i != 0 {
            sleep(INTER_DRAWING_PACE).await;
        }
        paint(app, x).await;
    }
}

#[named]
async fn paint_scrolls(app: &mut ApplicationContext<'_>) -> Result<()> {
    for fpath in env::args().skip(1) {
        debug!(target:function_name!(), "opening {fpath}...");
        match fpath {
            _ if fpath.ends_with(".jsonl") => {
                let mut ring = AllocRingBuffer::new(37);

                for d in serde_jsonlines::json_lines(fpath)? {
                    let d: DrawingBis = d?;
                    let d: Drawing = d.into();
                    let c = d.color();

                    info!(target:function_name!(), "{act} XxYxPxW: {x}x{y}x{p}x{w}",
                        act = if c==drawing::Color::Black {"drawing"} else {"erasing"},
                        x = d.xs.len(),
                        y = d.ys.len(),
                        p = d.pressures.len(),
                        w = d.widths.len(),
                    );

                    assert_eq!(
                        HashSet::from([d.xs.len()]),
                        HashSet::from([d.ys.len(), d.pressures.len(), d.widths.len()])
                    );

                    sleep(if true { DRAWING_PACE } else { INTER_DRAWING_PACE }).await;
                    paint(app, &d).await;

                    if c == drawing::Color::Black {
                        ring.enqueue(d.clone());
                    }
                    while ring.is_full() {
                        let Some(x) = ring.dequeue() else { break };
                        let x = Drawing { color: drawing::Color::White.into(), ..x };
                        paint(app, &x).await;
                    }
                }

                for x in ring.drain() {
                    let x = Drawing { color: drawing::Color::White.into(), ..x };
                    paint(app, &x).await;
                }
            }
            _ => error!(target:function_name!(), "No idea how to read {fpath}"),
        }
        sleep(DRAWING_PACE).await;
    }
    Ok(())
}

async fn paint_mouldings(app: &mut ApplicationContext<'_>) {
    let c = drawing::Color::Black;
    debug!("[paint_mouldings] drawing UI...");

    let mut parts = drawings::title_whiteboard::f(c);
    for part in &mut parts {
        for w in &mut part.widths {
            *w /= 2;
        }
    }
    paint_vec(app, &parts).await;
}

#[derive(Debug, Deserialize)]
pub struct DrawingBis {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub pressures: Vec<i32>,
    pub widths: Vec<u32>,
    pub color: String,
}

impl From<DrawingBis> for Drawing {
    fn from(DrawingBis { xs, ys, pressures, widths, color }: DrawingBis) -> Self {
        let color = match color.to_lowercase().as_ref() {
            "black" => drawing::Color::Black,
            "white" => drawing::Color::White,
            _ => drawing::Color::Invisible,
        } as i32;
        Self { xs, ys, pressures, widths, color }
    }
}

#[test]
fn reads_a_drawing_from_jsonl() {
    #[inline]
    fn d() -> DrawingBis {
        DrawingBis {
            xs: [
                99.82016, 99.82016, 99.90944, 100.08801, 100.17729, 100.355865, 100.62372,
                100.802284, 101.159424, 101.60585, 102.05227, 102.58798, 103.0344, 103.57011,
                104.10582, 104.730804, 105.53437,
            ]
            .into(),
            ys: [
                72.14423, 73.126396, 74.10855, 74.91214, 75.71573, 76.43003, 77.23361, 77.85863,
                78.39435, 78.93008, 79.287224, 79.733665, 80.1801, 80.62653, 81.072975, 81.43012,
                81.78727,
            ]
            .into(),
            widths: [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2].into(),
            pressures: [
                2326, 2294, 2244, 2191, 2149, 2083, 1989, 1899, 1794, 1674, 1542, 1420, 1330, 1256,
                1228, 1223, 20,
            ]
            .into(),
            color: "BLACK".into(),
        }
    }

    let p: Drawing = d().into();

    assert_eq!(d().xs, p.xs);
    assert_eq!(d().ys, p.ys);
    assert_eq!(d().pressures, p.pressures);
    assert_eq!(d().widths, p.widths);
    assert_eq!(drawing::Color::Black, p.color());
}
