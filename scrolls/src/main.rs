use std::time::Duration;

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
use log::{debug, info};
use marauder::{
    drawings,
    proto::hypercards::{drawing, Drawing},
};
use tokio::{spawn, task::spawn_blocking, time::sleep};

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

async fn paint(app: &mut ApplicationContext<'_>, drawing: Drawing) {
    let col = match drawing.color() {
        drawing::Color::White => color::WHITE,
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

async fn paint_vec(app: &mut ApplicationContext<'_>, xs: Vec<Drawing>) {
    for (i, x) in xs.into_iter().enumerate() {
        if i != 0 {
            sleep(INTER_DRAWING_PACE).await;
        }
        paint(app, x).await;
    }
}

async fn paint_mouldings(app: &mut ApplicationContext<'_>) {
    let c = drawing::Color::Black;
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
        ys: vec![TOOLBAR_HEIGHT as f32 - TOOLBAR_BAR_WIDTH as f32; count],
        pressures: vec![3992; count],
        widths: vec![TOOLBAR_BAR_WIDTH; count],
        color: c as i32,
    }
}
