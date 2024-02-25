use std::{collections::HashSet, env};

use anyhow::Result;
use libremarkable::appctx::ApplicationContext;
use log::info;
use pb::proto::hypercards::{drawing::Color, Drawing};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::Deserialize;
use tokio::time::sleep;

use crate::paint::{paint, DRAWING_PACE, INTER_DRAWING_PACE};

const PAUSE: bool = true;
const SYNC: bool = false;

pub(crate) async fn read_and_paint(app: &mut ApplicationContext<'_>, fpath: String) -> Result<()> {
    let mut ring = AllocRingBuffer::new(37);

    for d in serde_jsonlines::json_lines(fpath)? {
        let d: DrawingBis = d?;
        let d: Drawing = d.into();
        let c = d.color();

        info!(target:env!("CARGO_PKG_NAME"), "{act} XxYxPxW: {x}x{y}x{p}x{w}",
            act = if c==Color::Black {"drawing"} else {"erasing"},
            x = d.xs.len(),
            y = d.ys.len(),
            p = d.pressures.len(),
            w = d.widths.len(),
        );

        assert_eq!(
            HashSet::from([d.xs.len()]),
            HashSet::from([d.ys.len(), d.pressures.len(), d.widths.len()])
        );

        if PAUSE {
            sleep(if true { DRAWING_PACE } else { INTER_DRAWING_PACE }).await;
        }
        paint(app, &d, PAUSE, SYNC).await;

        if c == Color::Black {
            ring.enqueue(d.clone());
        }
        while ring.is_full() {
            let Some(x) = ring.dequeue() else { break };
            let x = Drawing { color: Color::White.into(), ..x };
            paint(app, &x, PAUSE, SYNC).await;
        }
    }

    for x in ring.drain() {
        let x = Drawing { color: Color::White.into(), ..x };
        paint(app, &x, PAUSE, SYNC).await;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(crate) struct DrawingBis {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub pressures: Vec<i32>,
    pub widths: Vec<u32>,
    pub color: String,
}

impl From<DrawingBis> for Drawing {
    fn from(DrawingBis { xs, ys, pressures, widths, color }: DrawingBis) -> Self {
        let color = match color.to_lowercase().as_ref() {
            "black" => Color::Black,
            "white" => Color::White,
            _ => Color::Invisible,
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
    assert_eq!(Color::Black, p.color());
}
