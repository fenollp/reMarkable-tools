use std::{
    fmt,
    sync::atomic::{AtomicI32, Ordering},
};

use libremarkable::{
    cgmath::Point2,
    framebuffer::common::mxcfb_rect,
    input::{Finger, MultitouchEvent},
};
use log::info;

pub struct Button {
    name: &'static str,
    area: mxcfb_rect,
    inner: AtomicI32,
}

impl fmt::Debug for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "btn:{}<{}>", self.name, self.inner.load(Ordering::Relaxed))
    }
}

impl Button {
    #[inline]
    #[must_use]
    pub fn new(nth: u8, name: &'static str) -> Self {
        let area = mxcfb_rect {
            top: 0, // y
            left: 100 * u32::from(nth),
            width: 100,
            height: 60, // y
        };
        Self { name, area, inner: AtomicI32::new(-1) }
    }

    #[inline]
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.inner.load(Ordering::Relaxed) != -1 // -1=off
    }

    // Returns whether button is /just now/ pressed.
    #[inline]
    #[must_use]
    fn press(&self, tracking_id: i32) -> bool {
        assert!(tracking_id != -1, "bad btn press {}", self.name);
        -1 == self.inner.swap(tracking_id, Ordering::Relaxed)
    }

    // Resets button if it's tracking `tracking_id`, returning `true` on success.
    #[inline]
    #[must_use]
    fn unpress(&self, tracking_id: i32) -> bool {
        assert!(tracking_id != -1, "bad btn unpress {}", self.name);
        let r = self.inner.compare_exchange(tracking_id, -1, Ordering::Relaxed, Ordering::Relaxed);
        r.is_ok()
    }

    #[inline]
    #[must_use]
    fn contains(&self, pos: &Point2<u32>) -> bool {
        self.area.contains_point(pos)
    }

    #[inline]
    pub fn process_event(&self, input: MultitouchEvent) {
        match input {
            MultitouchEvent::Press { finger } | MultitouchEvent::Move { finger } => {
                let Finger { tracking_id, pos, .. } = finger;
                let pos: Point2<u32> = (pos.x.into(), pos.y.into()).into();
                if self.contains(&pos) {
                    if self.press(tracking_id) {
                        info!("{self:?} just now pressed!");
                    }
                } else if self.unpress(tracking_id) {
                    info!("{self:?} reset!");
                }
            }
            MultitouchEvent::Release { finger: Finger { tracking_id, .. } } => {
                if self.unpress(tracking_id) {
                    info!("{self:?} reset");
                }
            }
            MultitouchEvent::Unknown => {}
        }
    }
}
