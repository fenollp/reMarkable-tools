use std::fmt;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TouchMode {
    OnlyUI,
    Bezier,
    Circles,
    Diamonds,
    FillDiamonds,
}

impl fmt::Display for TouchMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TouchMode::OnlyUI => write!(f, "None"),
            TouchMode::Bezier => write!(f, "Bezier"),
            TouchMode::Circles => write!(f, "Circles"),
            TouchMode::Diamonds => write!(f, "Diamonds"),
            TouchMode::FillDiamonds => write!(f, "FDiamonds"),
        }
    }
}

impl TouchMode {
    pub fn toggle(self) -> Self {
        match self {
            TouchMode::OnlyUI => TouchMode::Bezier,
            TouchMode::Bezier => TouchMode::Circles,
            TouchMode::Circles => TouchMode::Diamonds,
            TouchMode::Diamonds => TouchMode::FillDiamonds,
            TouchMode::FillDiamonds => TouchMode::OnlyUI,
        }
    }
}
