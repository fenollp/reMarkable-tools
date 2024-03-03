///! Replay files
///!
///! vfont https://news.ycombinator.com/item?id=39174421
///! vpdf https://willcrichton.net/notes/portable-epubs/#epub-content%2FEPUB%2Findex.xhtml$
///!
use std::{env, process::Command};

use anyhow::Result;
use libremarkable::appctx::{self, ApplicationContext};
use log::{debug, error, info};
use tokio::task::spawn_blocking;

mod jsonl;
mod ndjson;
mod paint;
mod svg;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut app: appctx::ApplicationContext<'_> = appctx::ApplicationContext::default();
    app.clear(true);
    app.draw_elements();

    let appref1 = app.upgrade_ref();
    spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(async move {
            if let Err(e) = paint_scrolls(appref1).await {
                error!(target:env!("CARGO_PKG_NAME"), "Error: {e}");
            }
            Command::new("systemctl").arg("start").arg("xochitl").spawn().unwrap();
            std::process::exit(0);
        });
    });

    info!("Init complete. Beginning event dispatch...");
    app.start_event_loop(false, true, false, |_ctx, evt| debug!("{evt:?}"));

    Ok(())
}

async fn paint_scrolls(app: &mut ApplicationContext<'_>) -> Result<()> {
    for fpath in env::args().skip(1) {
        debug!(target:env!("CARGO_PKG_NAME"), "opening {fpath}...");
        match fpath {
            _ if fpath.ends_with(".jsonl") => jsonl::read_and_paint(app, fpath).await?,
            _ if fpath.ends_with(".ndjson") => ndjson::read_and_paint(app, fpath).await?,
            _ if fpath.ends_with(".svg") => svg::read_and_paint(app, fpath).await?,
            _ => error!(target:env!("CARGO_PKG_NAME"), "No idea how to read {fpath}"),
        }
    }
    Ok(())
}
