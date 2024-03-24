//! Decode .ndjson files
//!
//! https://github.com/googlecreativelab/quickdraw-dataset
//! https://github.com/googlecreativelab/quickdraw-dataset#projects-using-the-dataset
//! https://magenta.tensorflow.org/sketch_rnn
//! https://github.com/googlecreativelab/quickdraw-dataset/issues/19#issuecomment-402247262
//! https://www.wikiwand.com/fr/Algorithme_de_Knuth-Morris-Pratt
//!
use std::{env, iter::repeat, time::Duration};

use anyhow::Result;
use libremarkable::{
    appctx::ApplicationContext,
    dimensions::{DISPLAYHEIGHT, DISPLAYWIDTH},
};
use log::info;
use pb::proto::hypercards::{drawing::Color, Drawing};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::Deserialize;
use tokio::time::sleep;

use crate::paint::paint;

#[test]
fn sizes() {
    assert_eq!(DISPLAYHEIGHT / 100, 18);
    assert_eq!(DISPLAYWIDTH / 75, 18);
}

pub(crate) async fn read_and_paint(app: &mut ApplicationContext<'_>, fpath: String) -> Result<()> {
    const W: f32 = 2. * 75.;
    const H: f32 = 2. * 100.;
    const COLS: f32 = (DISPLAYWIDTH as f32 / W) * 2. - 1. - 1.;
    const ROWS: f32 = (DISPLAYHEIGHT as f32 / H) * 2. - 1.;

    const PAUSE: Duration = Duration::from_millis(50);
    const SYNC: bool = false;

    let mut ring = AllocRingBuffer::new((COLS / 2.) as usize);

    for (i, ds) in serde_jsonlines::json_lines(fpath)?.enumerate() {
        let ds: DrawingBis = ds?;
        let ds: Vec<Drawing> = ds.into_vec();

        let i = i as f32;
        for d in ds.into_iter() {
            let c = d.color();

            info!(target:env!("CARGO_PKG_NAME"), "drawing x:{} y:{} XxY: {x}x{y}",
                (i / ROWS) % COLS,
                i % ROWS,
                x = d.xs.len(),
                y = d.ys.len(),
            );

            let d = Drawing {
                xs: d.xs.into_iter().map(|x| 0.5 * x + W * ((i / ROWS) % COLS)).collect(),
                ys: d.ys.into_iter().map(|y| 0.5 * y + H * (i % ROWS)).collect(),
                ..d
            };

            paint(app, &d, true, SYNC).await;
            sleep(PAUSE).await;

            if c == Color::Black {
                ring.enqueue(d.clone());
            }
            while ring.is_full() {
                let Some(x) = ring.dequeue() else { break };
                let x = Drawing { color: Color::White.into(), ..x };
                paint(app, &x, true, SYNC).await;
                // sleep(PAUSE).await;
            }
        }
    }

    for x in ring.drain() {
        let x = Drawing { color: Color::White.into(), ..x };
        paint(app, &x, true, SYNC).await;
        sleep(PAUSE).await;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(crate) struct DrawingBis {
    #[allow(dead_code)]
    pub word: String,

    #[allow(dead_code)]
    pub countrycode: String,

    #[allow(dead_code)]
    pub timestamp: String, // prolly chrono::datetime

    pub recognized: bool,

    #[allow(dead_code)]
    pub key_id: String, // stringified bigint

    pub drawing: Vec<[Vec<u8>; 2]>,
}

const PRESSURE: i32 = 2000;
const WIDTH: u32 = 2;

impl DrawingBis {
    fn into_vec(self) -> Vec<Drawing> {
        let DrawingBis { recognized, drawing, .. } = self;
        if !recognized {
            return vec![];
        }
        drawing
            .into_iter()
            .map(|[xs, ys]| {
                let n = xs.len();
                assert_eq!(n, ys.len());
                Drawing {
                    xs: xs.into_iter().map(f32::from).collect(),
                    ys: ys.into_iter().map(f32::from).collect(),
                    pressures: repeat(PRESSURE).take(n).collect(),
                    widths: repeat(WIDTH).take(n).collect(),
                    color: Color::Black as i32,
                }
            })
            .collect()
    }
}

#[test]
fn reads_a_drawing_from_jsonl() {
    #[inline]
    fn d() -> DrawingBis {
        DrawingBis {
            word: "tornado".into(),
            countrycode: "US".into(),
            timestamp: "2017-03-06 19:26:57.74406 UTC".into(),
            recognized: true,
            key_id: "6522189199179776".into(),
            drawing: vec![
                [
                    vec![
                        136, 114, 99, 54, 46, 65, 112, 150, 180, 180, 174, 151, 89, 6, 0, 6, 28,
                        77, 179, 184, 184, 178,
                    ],
                    vec![
                        0, 3, 8, 29, 36, 43, 51, 47, 32, 26, 19, 11, 15, 35, 38, 45, 56, 64, 70,
                        64, 57, 28,
                    ],
                ],
                [vec![1, 53, 84, 106], vec![39, 137, 183, 255]],
                [vec![106, 148, 160, 176, 178], vec![255, 158, 98, 60, 45]],
            ],
        }
    }

    let p: Vec<Drawing> = d().into_vec();

    assert_eq!(p.len(), 3);
    for d in p {
        let n = d.xs.len();
        assert_ne!(n, 0);
        assert_eq!(n, d.xs.len());
        assert_eq!(n, d.ys.len());
        assert_eq!(n, d.pressures.len());
        assert_eq!(n, d.widths.len());
        assert_eq!(Color::Black, d.color());
    }
}
