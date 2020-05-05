#![allow(clippy::many_single_char_names)]

use libloading::Symbol;
use image::{ImageBuffer, Rgb};
use retro_rs::{Emulator, Buttons};
use std::collections::HashSet;
use std::path::Path;
use itertools::Itertools;
mod sprites;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollLatch {
    V,
    H
}

impl Default for ScrollLatch {
    fn default() -> Self {
        Self::V
    }
}

impl ScrollLatch {
    fn clear()->Self {
        Self::V
    }
    fn flip(self)->Self {
        match self {
            Self::V => Self::H,
            Self::H => Self::V
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
#[allow(dead_code)]
enum ScrollChangeReason {
    Write2005, Write2006, Read2002
}

impl Default for ScrollChangeReason {
    fn default() -> Self {
        Self::Read2002
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
struct ScrollChange {
    reason:ScrollChangeReason,
    scanline:u8,
    value:u8
}

use std::hash::{Hash, Hasher};
#[derive(Clone)]
struct Tile([u8;8*8]);

impl PartialEq for Tile {
    fn eq(&self, other:&Self) -> bool {
        for (a,b) in self.0.iter().zip(other.0.iter()) {
            if a != b { return false; }
        }
        true
    }
}
impl Eq for Tile {}
impl Hash for Tile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

struct Framebuffer{fb:Vec<u8>, w:usize}

fn read_tile(x:usize,y:usize,fb:&Framebuffer) -> Tile {
    let mut tile = Tile([0_u8;64]);
    for yi in y..y+8 {
        for xi in x..x+8 {
            tile.0[(((yi-y) as u8)*8+((xi-x) as u8)) as usize] = fb.fb[fb.w*yi+xi];
        }
    }
    tile
}

// TODO return list of overlapping sprites
fn overlapping_sprite(x:usize, y:usize, sprites:&[sprites::SpriteData]) -> bool {
    for s in sprites {
        if !s.is_valid() { continue; }
        if x <= s.x as usize + 8 && s.x as usize <= x + 8 && y <= s.y as usize+s.height() as usize && s.y as usize <= y+8 {
            return true;
        }
    }
    false
}

fn find_tiling(lo:u8, hi:u8, (last_sx,last_sy):(u8,u8), fb:&Framebuffer, sprites:&[sprites::SpriteData], tiles:&mut HashSet<Tile>) -> (u8,u8) {
    let w = fb.w;
    let mut sx = 0;
    let mut sy = 0;
    let mut least_candidates : HashSet<Tile> = HashSet::new();
    let mut least_candidate_count = (w as usize*((hi-lo) as usize))/64;
    let mut candidates : HashSet<Tile> = HashSet::with_capacity((w as usize*((hi-lo) as usize))/64);
    let mut offsets:Vec<_> = (0..8_usize).cartesian_product(0..8_usize).collect();
    offsets.sort_by_key(|&(x,y)| {
        (x as i32-last_sx as i32).abs() + (y as i32-last_sy as i32).abs()
    });
    // remove possible junk lines
    let lo = lo.max(8) as usize;
    let hi = hi.min(240-8) as usize;
    'offset: for (xo,yo) in offsets {
        let mut checks = 0;
        for y in ((lo+yo)..hi).step_by(8) {
            // bring in both sides by 8 to avoid junk pixels
            for x in ((xo+8)..(w-8)).step_by(8) {
                if overlapping_sprite(x,y,sprites) { continue; }
                checks+=1;
                let tile = read_tile(x as usize, y as usize, &fb);
                if tiles.contains(&tile) { continue; }
                candidates.insert(tile);
                if candidates.len() > least_candidate_count {
                    // println!("Skip {:?} bc {:?} new candidates, vs {:?}", (xo,yo), candidates.len(), least_candidate_count);
                    candidates.clear();
                    continue 'offset;
                }
            }
        }
        if candidates.len() < least_candidate_count {
            println!("Found {:?} has {:?} candidates after {:?} checks", (xo,yo), candidates.len(), checks);
            least_candidate_count = candidates.len();
            least_candidates = candidates.clone();
            sx = xo as u8;
            sy = yo as u8;
            candidates.clear();
        }
    }
    tiles.extend(least_candidates);
    (sx,sy)
}

#[derive(Default, Clone)]
struct MappyState {
    latch : ScrollLatch,
    tiles : HashSet<Tile>,
    scroll : (u8,u8),
}

fn get_changes(emu:&mut emu, changes:&mut Vec<ScrollChange>, change_count:&mut u32) {
    let get_changes_fn:Symbol<unsafe extern fn(*mut ScrollChange, u32) -> u32> = emu.get_symbol(b"retro_count_scroll_changes").unwrap();
    unsafe {
        *change_count = get_changes_fn(changes.as_mut_ptr(), 0);
        changes.resize_with(*change_count as usize, Default::default);
        get_changes_fn(changes.as_mut_ptr(), *change_count);
    }
}

fn get_splits(mappy:&mut MappyState, changes:&Vec<ScrollChange>) -> Vec<u8> {
    let mut splits:Vec<u8> = vec![];
    splits.push(0);
    for &ScrollChange { reason, scanline, .. } in changes.iter() {
        let scanline = if scanline < 240 {scanline} else {0};
        // let old_latch = latch;
        let maybe_change = match reason {
            ScrollChangeReason::Read2002 => {
                mappy.latch = ScrollLatch::clear();
                false
            },
            ScrollChangeReason::Write2005 => {
                mappy.latch = mappy.latch.flip();
                true
            },
            ScrollChangeReason::Write2006 => {
                mappy.latch = mappy.latch.flip();
                true
            },
        };
        if maybe_change && splits[splits.len()-1] < scanline {
            // Don't want to use the line where scrolling changed
            splits.push(scanline+1);
        }
    }
    if splits[splits.len()-1] < 240 {
        splits.push(240);
    }
    splits
}

fn main() {
    let mut emu = Emulator::create(
        Path::new("../cores/fceumm_libretro"),
        Path::new("../roms/mario.nes"),
    );
    let mut sprites = [sprites::SpriteData::default(); sprites::SPRITE_COUNT];

    let mut changes:Vec<ScrollChange> = Vec::with_capacity(32000);
    let mut change_count:u32;
    let mut mappy = MappyState { tiles:HashSet::with_capacity(1024), ..Default::default()};
    emu.run([Buttons::new(), Buttons::new()]);
    let (w,h) = emu.framebuffer_size();
    let mut fb = Framebuffer{fb:vec![0;w*h], w};
    for _ in 0..50 {
        emu.run([Buttons::new(), Buttons::new()]);
        // TODO: make fb.fb work on u64s for 8 pixel spans?  measure!
        emu.for_each_pixel(|x,y,r,g,b| {
            let r = ((r as u32 * 8) / 256) as u8;
            let g = ((g as u32 * 8) / 256) as u8;
            let b = ((b as u32 * 4) / 256) as u8;
            assert!(r <= 7);
            assert!(g <= 7);
            assert!(b <= 3);
            fb.fb[y*fb.w+x] = (r << 5) + (g << 2) + b;
        }).expect("Couldn't get FB");
        get_changes(&mut emu, &mut changes, &mut change_count);
        let splits = get_splits(&mut mappy, &changes);
        sprites::get_sprites(&emu, &mut sprites);
        dbg!(sprites.iter().filter(|s| s.is_valid()).count());
        // We are trying to find a tiling that reuses existing tiles, or
        // a minimal tiling otherwise.
        if let Some(win) = splits.windows(2).max_by_key(|&win| {
            match win {
                [lo,hi] => hi-lo,
                _ => panic!("Misshapen windows")
            }
        }) {
            match *win {
                [lo,hi] => {
                    mappy.scroll = find_tiling(lo, hi, mappy.scroll, &fb, &sprites, &mut mappy.tiles);
                    println!("Scroll: {:?}--{:?} : {:?}",lo,hi,mappy.scroll);
                    println!("Known tiles: {:?}", mappy.tiles.len());
                },
                _ => panic!("Misshapen windows")
            }
        }
    }
    for (ti,tile) in mappy.tiles.iter().enumerate() {
        let mut buf = vec![0_u8;8*8*3];
        for yi in 0..8 {
            for xi in 0..8 {
                let col = tile.0[yi*8+xi] as u32;
                let r = (((col & 0b1110_0000) >> 5) * 255) / 8;
                let g = (((col & 0b0001_1100) >> 2) * 255) / 8;
                let b = ((col & 0b0000_0011) * 255) / 4;
                assert!(r <= 255);
                assert!(g <= 255);
                assert!(b <= 255);
                buf[(yi*8+xi)*3] = r as u8;
                buf[(yi*8+xi)*3+1] = g as u8;
                buf[(yi*8+xi)*3+2] = b as u8;
            }
        }
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(8, 8, &buf[..]).expect("Couldn't create image buffer");
        img.save(format!("../out/t{:?}.png", ti)).unwrap();
    }
    println!("Hello, world!");
}
