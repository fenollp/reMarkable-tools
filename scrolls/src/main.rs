use std::{collections::HashSet, env, process::Command, time::Duration};

use anyhow::Result;
use libremarkable::{
    appctx,
    appctx::ApplicationContext,
    framebuffer::{
        cgmath,
        cgmath::EuclideanSpace,
        common::{color, display_temp, dither_mode, waveform_mode, DRAWING_QUANT_BIT_3},
        FramebufferDraw, FramebufferRefresh, PartialRefreshMode,
    },
    // ui_extensions::element::{UIConstraintRefresh, UIElement, UIElementWrapper},
};
use log::{debug, error, info};
use pb::proto::hypercards::{drawing, Drawing};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::Deserialize;
use tokio::{task::spawn_blocking, time::sleep};

// const TOOLBAR_BAR_WIDTH: u32 = 2;
// const TOOLBAR_HEIGHT: u32 = 70 + TOOLBAR_BAR_WIDTH;
// const CANVAS_REGION: mxcfb_rect = mxcfb_rect {
//     top: TOOLBAR_HEIGHT,
//     left: 0,
//     height: DISPLAYHEIGHT as u32 - TOOLBAR_HEIGHT,
//     width: DISPLAYWIDTH as u32,
// };

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();
    app.clear(true);
    // app.add_element(
    //     "canvasRegion",
    //     UIElementWrapper {
    //         position: CANVAS_REGION.top_left().cast().unwrap()
    //             + cgmath::vec2(0, -(TOOLBAR_BAR_WIDTH as i32)),
    //         refresh: UIConstraintRefresh::RefreshAndWait,
    //         inner: UIElement::Region {
    //             size: CANVAS_REGION.size().cast().unwrap() + cgmath::vec2(1, 3),
    //             border_px: 0,
    //             border_color: color::BLACK,
    //         },
    //         ..Default::default()
    //     },
    // );
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
                error!(target:env!("CARGO_PKG_NAME"), "Error: {e}");
            }
            Command::new("systemctl").arg("start").arg("xochitl").spawn().unwrap();
            std::process::exit(0);
        });
    });

    info!("Init complete. Beginning event dispatch...");
    app.start_event_loop(false, false, false, |_ctx, evt| debug!("[event] {evt:?}"));

    Ok(())
}

async fn paint_scrolls(app: &mut ApplicationContext<'_>) -> Result<()> {
    for fpath in env::args().skip(1) {
        debug!(target:env!("CARGO_PKG_NAME"), "opening {fpath}...");
        match fpath {
            _ if fpath.ends_with(".jsonl") => jsonl::read_and_paint(app, fpath).await?,
            _ if fpath.ends_with(".ndjson") => read_and_paint_ndjson(app, fpath).await?,
            _ => error!(target:env!("CARGO_PKG_NAME"), "No idea how to read {fpath}"),
        }
        sleep(DRAWING_PACE).await;
    }
    Ok(())
}

async fn read_and_paint_ndjson(app: &mut ApplicationContext<'_>, fpath: String) -> Result<()> {
    // let mut ring = AllocRingBuffer::new(37);

    // for d in serde_jsonlines::json_lines(fpath)? {
    //     let d: DrawingBis = d?;
    //     let d: Drawing = d.into();
    //     let c = d.color();

    //     info!(target:env!("CARGO_PKG_NAME"), "{act} XxY: {x}x{y}",
    //         act = if c==drawing::Color::Black {"drawing"} else {"erasing"},
    //         x = d.xs.len(),
    //         y = d.ys.len(),
    //         p = d.pressures.len(),
    //         w = d.widths.len(),
    //     );

    //     assert_eq!(
    //         HashSet::from([d.xs.len()]),
    //         HashSet::from([d.ys.len(), d.pressures.len(), d.widths.len()])
    //     );

    //     sleep(if true { DRAWING_PACE } else { INTER_DRAWING_PACE }).await;
    //     paint(app, &d).await;

    //     if c == drawing::Color::Black {
    //         ring.enqueue(d.clone());
    //     }
    //     while ring.is_full() {
    //         let Some(x) = ring.dequeue() else { break };
    //         let x = Drawing { color: drawing::Color::White.into(), ..x };
    //         paint(app, &x).await;
    //     }
    // }

    // for x in ring.drain() {
    //     let x = Drawing { color: drawing::Color::White.into(), ..x };
    //     paint(app, &x).await;
    // }
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

async fn paint_vec(app: &mut ApplicationContext<'_>, xs: &[Drawing]) {
    for (i, x) in xs.iter().enumerate() {
        if i != 0 {
            sleep(INTER_DRAWING_PACE).await;
        }
        paint(app, x).await;
    }
}
