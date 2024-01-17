use std::{env, iter::repeat};

use anyhow::Result;
use libremarkable::appctx::ApplicationContext;
use log::info;
use pb::proto::hypercards::{drawing, Drawing};
use ringbuffer::{AllocRingBuffer, RingBuffer};
use serde::Deserialize;
use tokio::time::sleep;

use crate::paint::{paint, DRAWING_PACE, INTER_DRAWING_PACE};

pub(crate) async fn read_and_paint(app: &mut ApplicationContext<'_>, fpath: String) -> Result<()> {
    let mut ring = AllocRingBuffer::new(37);

    for (i, ds) in serde_jsonlines::json_lines(fpath)?.enumerate() {
        let ds: DrawingBis = ds?;
        let ds: Vec<Drawing> = ds.into_vec();

        let (off_x, off_y) = (150f32, 200f32);
        let (mul_x, mul_y) = (0.5f32, 0.5f32);
        let i = i as f32;
        for d in ds.into_iter() {
            let c = d.color();

            info!(target:env!("CARGO_PKG_NAME"), "drawing XxY: {x}x{y}",
                x = d.xs.len(),
                y = d.ys.len(),
            );

            let d = Drawing {
                xs: d.xs.into_iter().map(|x| (x + off_x * i) * mul_x).collect(),
                ys: d.ys.into_iter().map(|y| (y + off_y * i) * mul_y).collect(),
                ..d
            };

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
    }

    for x in ring.drain() {
        let x = Drawing { color: drawing::Color::White.into(), ..x };
        paint(app, &x).await;
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
                    color: drawing::Color::Black as i32,
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
        assert_eq!(drawing::Color::Black, d.color());
    }
}
