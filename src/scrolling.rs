#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollLatch {
    H,
    V,
}

impl Default for ScrollLatch {
    fn default() -> Self {
        Self::H
    }
}

impl ScrollLatch {
    pub fn clear() -> Self {
        Self::H
    }
    pub fn flip(self) -> Self {
        match self {
            Self::H => Self::V,
            Self::V => Self::H,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
#[allow(dead_code)]
pub enum ScrollChangeReason {
    Write2005,
    Write2006,
    Read2002,
}

impl Default for ScrollChangeReason {
    fn default() -> Self {
        Self::Read2002
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ScrollChange {
    pub reason: ScrollChangeReason,
    pub scanline: u8,
    pub value: u8,
}
