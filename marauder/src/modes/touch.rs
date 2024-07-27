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

impl From<TouchMode> for u8 {
    fn from(mode: TouchMode) -> Self {
        match mode {
            TouchMode::OnlyUI => 1,
            TouchMode::Bezier => 2,
            TouchMode::Circles => 3,
            TouchMode::Diamonds => 4,
            TouchMode::FillDiamonds => 5,
        }
    }
}

impl From<u8> for TouchMode {
    fn from(mode: u8) -> Self {
        match mode {
            1 => Self::OnlyUI,
            2 => Self::Bezier,
            3 => Self::Circles,
            4 => Self::Diamonds,
            5 => Self::FillDiamonds,
            _ => panic!("Unmapped mode value: {mode}"),
        }
    }
}
