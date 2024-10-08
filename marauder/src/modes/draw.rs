use std::fmt;

#[derive(Copy, Clone, PartialEq)]
pub enum DrawMode {
    Draw(u32),
    Erase(u32),
}

impl Default for DrawMode {
    fn default() -> Self {
        Self::Draw(2)
    }
}

impl fmt::Display for DrawMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DrawMode::Draw(_) => write!(f, "Black"),
            DrawMode::Erase(_) => write!(f, "White"),
        }
    }
}

impl DrawMode {
    pub fn set_size(self, new_size: u32) -> Self {
        match self {
            DrawMode::Draw(_) => DrawMode::Draw(new_size),
            DrawMode::Erase(_) => DrawMode::Erase(new_size),
        }
    }

    pub fn get_size(self) -> u32 {
        match self {
            DrawMode::Draw(s) => s,
            DrawMode::Erase(s) => s,
        }
    }
}

impl From<DrawMode> for i32 {
    fn from(mode: DrawMode) -> Self {
        match mode {
            DrawMode::Draw(s) => s as i32,
            DrawMode::Erase(s) => -(s as i32),
        }
    }
}

impl From<i32> for DrawMode {
    fn from(mode: i32) -> Self {
        if mode >= 0 {
            Self::Draw(mode as u32)
        } else {
            Self::Erase((-mode) as u32)
        }
    }
}
