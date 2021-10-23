use super::scrolling::*;
use crate::framebuffer::Framebuffer;
use crate::tile::TILE_SIZE;
use crate::Rect;
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Split {
    pub scanline: u8,
    pub scroll_x: u8,
    pub scroll_y: u8,
}

fn register_split(splits: &mut Vec<Split>, scanline: u8) {
    let last = &splits[splits.len() - 1];
    if last.scanline < scanline {
        let scroll_x = last.scroll_x;
        let scroll_y = last.scroll_y;
        splits.push(Split {
            scanline,
            scroll_x,
            scroll_y,
        });
    }
}

/// Tries to find a rectangle of a solid-colored border starting from `start` and moving by `dir`.
// TODO : move to framebuffer?
pub fn skim_rect(fb: &Framebuffer, start: i16, dir: i16) -> u8 {
    let color = fb.fb[start as usize * fb.w];
    for column in 0..fb.w {
        if fb.fb[start as usize * fb.w + column] != color {
            return 0;
        }
    }
    let mut row = start;
    let mut last_good_row = start;
    while 0 <= row && row < 240 {
        let left = fb.fb[row as usize * fb.w];
        let right = fb.fb[row as usize * fb.w + fb.w - 1];
        if left != right {
            break;
        }
        if fb.fb[row as usize * fb.w..(row as usize * fb.w + 1)]
            .iter()
            .all(|here| *here == color)
        {
            last_good_row = row;
        }
        row += dir;
    }
    (last_good_row + 1 - start).abs() as u8
}

pub fn get_splits(changes: &[ScrollChange], mut latch: ScrollLatch) -> (Vec<Split>, ScrollLatch) {
    let mut splits = vec![Split {
        scanline: 0,
        scroll_x: 0,
        scroll_y: 0,
    }];
    for &ScrollChange {
        reason,
        scanline,
        value,
    } in changes.iter()
    {
        let scanline = if scanline < 240 { scanline } else { 0 };
        // let old_latch = latch;
        match reason {
            ScrollChangeReason::Read2002 => {
                latch = ScrollLatch::clear();
            }
            ScrollChangeReason::Write2005 => {
                register_split(&mut splits, scanline + 1);
                let last = splits.len() - 1;
                match latch {
                    ScrollLatch::H => {
                        splits[last].scroll_x = value;
                    }
                    ScrollLatch::V => {
                        splits[last].scroll_y = value;
                    }
                };
                latch = latch.flip();
            }
            ScrollChangeReason::Write2006 => {
                let scanline = if scanline > 3 { scanline - 3 } else { scanline };
                register_split(&mut splits, scanline + 1);
                let last = splits.len() - 1;
                match latch {
                    ScrollLatch::H => {
                        // First byte of 15-bit PPUADDR:
                        // [] yyy NN YY
                        // Of the first byte written to 2006:
                        // bits 0 and 1 are ignored, rest mapped to yyNNYY
                        // (and the leftmost bit of y_fine is forced to 0)
                        let y_fine = (value & 0b0011_0000) >> 4;
                        // (ignore nametable select NN for now)
                        // two highest bits of y_coarse are written
                        let y_coarse_hi = (value & 0b0000_0011) << 6;
                        // combine that with the three middle bits of old y scroll
                        let y_coarse = y_coarse_hi | (splits[last].scroll_y & 0b00111000);
                        splits[last].scroll_y = y_coarse | y_fine;
                    }
                    ScrollLatch::V => {
                        // Second byte of PPUADDR:
                        // YYYX XXXX
                        let y_coarse_lo = (value & 0b1110_0000) >> 2;
                        let x_coarse = (value & 0b0001_1111) << 3;
                        // overwrite middle three bits of old y scroll
                        let kept_y = splits[last].scroll_y & 0b1100_0111;
                        // overwrite left five bits of old x scroll
                        let kept_x = splits[last].scroll_x & 0b0000_0111;
                        splits[last].scroll_x = x_coarse | kept_x;
                        splits[last].scroll_y = kept_y | y_coarse_lo;
                    }
                };
                latch = latch.flip();
            }
        };
    }
    if splits[splits.len() - 1].scanline < 240 {
        splits.push(Split {
            scanline: 240,
            scroll_x: 0,
            scroll_y: 0,
        });
    }
    (splits, latch)
}

fn get_best_effort_splits(fb: &Framebuffer, lo: Split, hi: Split) -> (Split, Split) {
    //If we can skim a rectangle bigger than 24px high at the top or the bottom, our split is height - that
    let down_skim_len = skim_rect(&fb, 0, 1);
    let up_skim_len = skim_rect(&fb, 239, -1);
    let mut s0 = lo;
    let mut s1 = hi;
    if down_skim_len >= 24 && down_skim_len < 120 {
        //move the top split lower
        s0.scanline = down_skim_len;
    }
    if up_skim_len >= 24 && up_skim_len < 120 {
        //move the bottom split higher
        s1.scanline = 240 - up_skim_len;
    }
    (s0, s1)
}

pub fn get_main_split(
    changes: &[ScrollChange],
    latch: ScrollLatch,
    fb: &Framebuffer,
) -> (Split, Split, ScrollLatch) {
    let (splits, latch) = get_splits(changes, latch);
    let (lo, hi) = {
        match splits.windows(2).max_by_key(|&win| match win {
            [lo, hi] => hi.scanline - lo.scanline,
            _ => panic!("Misshapen windows"),
        }) {
            Some(&[lo, hi]) => (lo, hi),
            _ => panic!("No valid splits"),
        }
    };
    // Is splitting happening some other way?
    // E.g. in Zelda the "room" abuts the "menu"
    if hi.scanline - lo.scanline >= 239 {
        let (lo_b, hi_b) = get_best_effort_splits(fb, lo, hi);
        // dbg!(("a", lo, hi, lo_b, hi_b, latch, changes.len()));
        (lo_b, hi_b, latch)
    } else {
        // dbg!(("b", lo,hi,latch,changes.len()));
        (lo, hi, latch)
    }
}
const SCREEN_SAFE_LEFT: u32 = 8;
const SCREEN_SAFE_RIGHT: u32 = 8;
const SCREEN_SAFE_TOP: u32 = 8;
const SCREEN_SAFE_BOTTOM: u32 = 8;
pub fn split_region_for(lo: u32, hi: u32, xo: u8, yo: u8, w: u32, h: u32) -> Rect {
    let lo = lo.max(SCREEN_SAFE_TOP);
    let hi = hi.min(h - SCREEN_SAFE_BOTTOM);
    let xo = ((TILE_SIZE - (xo as usize % TILE_SIZE)) % TILE_SIZE) as u32;
    let yo = ((TILE_SIZE - (yo as usize % TILE_SIZE)) % TILE_SIZE) as u32;
    let dy = hi - (lo + yo);
    let dy = (dy / (TILE_SIZE as u32)) * (TILE_SIZE as u32);
    let dx = (w as u32 - SCREEN_SAFE_RIGHT) - (xo + SCREEN_SAFE_LEFT);
    let dx = (dx / (TILE_SIZE as u32)) * (TILE_SIZE as u32);
    Rect::new(
        SCREEN_SAFE_LEFT as i32 + xo as i32,
        lo as i32 + yo as i32,
        dx,
        dy,
    )
}
