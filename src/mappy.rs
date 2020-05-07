use crate::framebuffer::Framebuffer;
use crate::scrolling::*;
use crate::sprites;
use crate::Rect;
use crate::tile::TileGfx;
use crate::screen::Screen;
use image::{ImageBuffer, Rgb};
use itertools::Itertools;
use libloading::Symbol;
use retro_rs::Emulator;
use std::collections::HashSet;
use std::path::Path;

pub struct MappyState {
    latch: ScrollLatch,
    pub tiles: HashSet<TileGfx>,
    pub grid_align: (u8, u8),
    pub scroll: (i16,i16),
    pub splits: [(u8, u8); 1],
    pub live_sprites: [sprites::SpriteData; sprites::SPRITE_COUNT],
    pub current_screen: Screen<TileGfx>,
    fb: Framebuffer,
    changes: Vec<ScrollChange>,
    change_count: u32,
}

impl MappyState {
    pub fn new(w: usize, h: usize) -> Self {
        MappyState {
            latch: ScrollLatch::default(),
            tiles: HashSet::with_capacity(1024),
            grid_align: (0, 0),
            scroll: (0,0),
            splits: [(0, 240)],
            live_sprites: [sprites::SpriteData::default(); sprites::SPRITE_COUNT],
            fb: Framebuffer::new(w, h),
            changes: Vec::with_capacity(32000),
            change_count: 0,
            current_screen: Screen::new(Rect::new(0,0,0,0))
        }
    }
    fn find_tiling(&mut self, lo: u8, hi: u8) {
        let (last_sx, last_sy) = self.grid_align;
        let w = self.fb.w;
        let mut sx = 0;
        let mut sy = 0;
        let mut least_candidates: HashSet<TileGfx> = HashSet::new();
        let mut least_candidate_count = (w as usize * ((hi - lo) as usize)) / 64;
        let mut candidates: HashSet<TileGfx> =
            HashSet::with_capacity((w as usize * ((hi - lo) as usize)) / 64);
        let mut offsets: Vec<_> = (0..8_u8).cartesian_product(0..8_u8).collect();
        offsets.sort_by_key(|&(x, y)| {
            (x as i32 - last_sx as i32).abs() + (y as i32 - last_sy as i32).abs()
        });
        'offset: for (xo, yo) in offsets {
            //let mut checks = 0
            let fb = &self.fb;
            let region = self.split_region_for(lo as u32, hi as u32, xo, yo);
            let tiles = &mut self.tiles;

            // let mut checks = 0;
            for y in (region.y as u32..(region.y as u32+region.h)).step_by(8) {
                for x in (region.x as u32..(region.x as u32+region.w)).step_by(8) {
                    if sprites::overlapping_sprite(x as usize, y as usize, &self.live_sprites) {
                        continue;
                    }
                    // checks += 1;
                    let tile = TileGfx::read(fb, x as usize, y as usize);
                    if tiles.contains(&tile) {
                        continue;
                    }
                    candidates.insert(tile);
                    if candidates.len() > least_candidate_count {
                        // println!("Skip {:?} bc {:?} new candidates, vs {:?}", (xo,yo), candidates.len(), least_candidate_count);
                        candidates.clear();
                        continue 'offset;
                    }
                }
            }
            if candidates.len() < least_candidate_count {
                // TODO flag to do this in batch mode?
                // println!("Found {:?} has {:?} candidates after {:?} checks", (xo,yo), candidates.len(), checks);
                least_candidate_count = candidates.len();
                least_candidates = candidates.clone();
                sx = xo;
                sy = yo;
                candidates.clear();
            }
        }
        let tiles = &mut self.tiles;
        tiles.extend(least_candidates);
        self.grid_align = (sx, sy);
    }
    fn get_splits(&mut self) -> Vec<u8> {
        let mut splits: Vec<u8> = vec![];
        splits.push(0);
        for &ScrollChange {
            reason, scanline, ..
        } in self.changes.iter()
        {
            let scanline = if scanline < 240 { scanline } else { 0 };
            // let old_latch = latch;
            let maybe_change = match reason {
                ScrollChangeReason::Read2002 => {
                    self.latch = ScrollLatch::clear();
                    false
                }
                ScrollChangeReason::Write2005 => {
                    self.latch = self.latch.flip();
                    true
                }
                ScrollChangeReason::Write2006 => {
                    self.latch = self.latch.flip();
                    true
                }
            };
            if maybe_change && splits[splits.len() - 1] < scanline {
                // Don't want to use the line where scrolling changed
                splits.push(scanline + 1);
            }
        }
        if splits[splits.len() - 1] < 240 {
            splits.push(240);
        }
        splits
    }
    pub fn process_screen(&mut self, emu: &Emulator) {
        self.fb.read_from(&emu);
        self.get_changes(&emu);

        sprites::get_sprites(&emu, &mut self.live_sprites);
        // We are trying to find a tiling that reuses existing tiles, or
        // a minimal tiling otherwise.
        let splits = self.get_splits();
        let (lo, hi) = {
            match splits.windows(2).max_by_key(|&win| match win {
                [lo, hi] => hi - lo,
                _ => panic!("Misshapen windows"),
            }) {
                Some(&[lo, hi]) => (lo, hi),
                _ => panic!("No valid splits"),
            }
        };
        self.splits = [(lo, hi)];
        let old_align = self.grid_align;
        self.find_tiling(lo, hi);
        // update scroll based on grid align change.
        self.scroll = (
            self.scroll.0 + find_offset(old_align.0, self.grid_align.0) as i16,
            self.scroll.1 + find_offset(old_align.1, self.grid_align.1) as i16
        );
        let region = self.split_region();
        self.current_screen = Screen::new(Rect::new(0, 0, region.w/8, region.h/8));
        for y in (region.y..(region.y+region.h as i32)).step_by(8) {
            for x in (region.x..(region.x+region.w as i32)).step_by(8) {
                if sprites::overlapping_sprite(x as usize, y as usize, &self.live_sprites) {
                    // Just leave the empty one there
                    continue;
                }
                let tile = TileGfx::read(&self.fb, x as usize, y as usize);
                if !(self.tiles.contains(&tile)) {
                    println!("Unaccounted-for tile, {},{} hash {}", (x-region.x)/8, (y-region.y)/8, tile.perceptual_hash());
                }
                self.current_screen.set(tile, (x-region.x) as u32/8, (y-region.y) as u32/8);
            }
        }
    }

    pub fn split_region_for(&self, lo:u32, hi:u32, xo:u8, yo:u8) -> Rect {
        let lo = lo.max(8);
        let hi = hi.min(self.fb.h as u32-8);
        let dy = hi - (lo+yo as u32);
        let dy = (dy/8)*8;
        let dx = (self.fb.w as u32 - 8) - (xo as u32+8);
        let dx = (dx/8)*8;
        Rect::new(
            8+xo as i32,
            lo as i32+yo as i32,
            dx,
            dy
        )
    }

    pub fn split_region(&self) -> Rect {
        // [src/mappy.rs:65] lo + yo = 32
        // [src/mappy.rs:65] hi = 232
        // [src/mappy.rs:65] xo + 8 = 8
        // [src/mappy.rs:65] w - 8 = 248
        self.split_region_for(self.splits[0].0 as u32, self.splits[0].1 as u32, self.grid_align.0, self.grid_align.1)
    }

    fn get_changes(&mut self, emu: &Emulator) {
        let get_changes_fn: Symbol<unsafe extern "C" fn(*mut ScrollChange, u32) -> u32> =
            emu.get_symbol(b"retro_count_scroll_changes").unwrap();
        unsafe {
            self.change_count = get_changes_fn(self.changes.as_mut_ptr(), 0);
            self.changes
                .resize_with(self.change_count as usize, Default::default);
            get_changes_fn(self.changes.as_mut_ptr(), self.change_count);
        }
    }
    pub fn dump_tiles(&self, root: &Path) {
        let mut buf = vec![0_u8; 8 * 8 * 3];
        for (ti, tile) in self.tiles.iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img = ImageBuffer::<Rgb<u8>, _>::from_raw(8, 8, &buf[..])
                .expect("Couldn't create image buffer");
            img.save(root.join(format!("t{:}.png", ti))).unwrap();
        }
    }
}

fn find_offset(old:u8, new:u8) -> i8 {
    // each coordinate either increased and possibly wrapped or decreased and possibly wrapped or stayed the same
    // in the former case calculate new+8 and subtract old if new < old, otherwise new - old
    // in the middle case calculate old+8 - new if new > old, otherwise old - new
    let old = 7-(old as i8);
    let new = 7-(new as i8);
    let decrease = if new <= old {
        new-old
    } else {
        new-(old+8)
    };
    let increase = if new >= old {
        new-old
    } else {
        (new+8)-old
    };

    *[decrease, increase].iter().min_by_key(|n| n.abs()).unwrap()
}
