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

pub fn find_offset(old: u8, new: u8) -> i16 {
    // each coordinate either increased and possibly wrapped or decreased and possibly wrapped or stayed the same
    // in the former case calculate new+8 and subtract old if new < old, otherwise new - old
    // in the middle case calculate old+8 - new if new > old, otherwise old - new
    // the magic number here (255, 8, whatever) is the largest value grid_offset can take
    let old = old as i16;
    let new = new as i16;
    let decrease = if new <= old {
        new - old
    } else {
        new - (old + 256)
    };
    let increase = if new >= old {
        new - old
    } else {
        (new + 256) - old
    };

    *[decrease, increase].iter().min_by_key(|n| n.abs()).unwrap()
}
