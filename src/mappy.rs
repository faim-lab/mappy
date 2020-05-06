use libloading::Symbol;
use image::{ImageBuffer, Rgb};
use retro_rs::{Emulator, Buttons};
use std::collections::HashSet;
use crate::scrolling::*;
use crate::framebuffer::Framebuffer;
use crate::tile::Tile;
use crate::sprites;
use itertools::Itertools;
use std::path::Path;

pub struct MappyState {
    latch : ScrollLatch,
    pub tiles : HashSet<Tile>,
    pub scroll : (u8,u8),
    pub splits : [(u8,u8);1],
    pub live_sprites : [sprites::SpriteData;sprites::SPRITE_COUNT],
    fb : Framebuffer,
    changes: Vec<ScrollChange>,
    change_count: u32
}

impl MappyState {
    pub fn new(w:usize, h:usize) -> Self {
        MappyState {
            latch:ScrollLatch::default(),
            tiles:HashSet::with_capacity(1024),
            scroll:(0,0),
            splits:[(0,240)],
            live_sprites:[sprites::SpriteData::default(); sprites::SPRITE_COUNT],
            fb:Framebuffer::new(w,h),
            changes: Vec::with_capacity(32000),
            change_count:0
        }
    }
    fn find_tiling(&mut self, lo:u8, hi:u8) {
        let (last_sx, last_sy) = self.scroll;
        let sprites = &self.live_sprites;
        let tiles = &mut self.tiles;
        let w = self.fb.w;
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
                    if sprites::overlapping_sprite(x,y,sprites) { continue; }
                    checks+=1;
                    let tile = Tile::read(&self.fb, x as usize, y as usize);
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
                // TODO flag to do this in batch mode?
                // println!("Found {:?} has {:?} candidates after {:?} checks", (xo,yo), candidates.len(), checks);
                least_candidate_count = candidates.len();
                least_candidates = candidates.clone();
                sx = xo as u8;
                sy = yo as u8;
                candidates.clear();
            }
        }
        tiles.extend(least_candidates);
        self.scroll = (sx,sy);
    }
    fn get_splits(&mut self) -> Vec<u8> {
        let mut splits:Vec<u8> = vec![];
        splits.push(0);
        for &ScrollChange { reason, scanline, .. } in self.changes.iter() {
            let scanline = if scanline < 240 {scanline} else {0};
            // let old_latch = latch;
            let maybe_change = match reason {
                ScrollChangeReason::Read2002 => {
                    self.latch = ScrollLatch::clear();
                    false
                },
                ScrollChangeReason::Write2005 => {
                    self.latch = self.latch.flip();
                    true
                },
                ScrollChangeReason::Write2006 => {
                    self.latch = self.latch.flip();
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
    pub fn process_screen(&mut self, emu:&Emulator) {
        self.fb.read_from(&emu);
        self.get_changes(&emu);

        sprites::get_sprites(&emu, &mut self.live_sprites);
        // We are trying to find a tiling that reuses existing tiles, or
        // a minimal tiling otherwise.
        let splits = self.get_splits();
        let (lo,hi) = {
            match splits.windows(2).max_by_key(|&win| {
                match win {
                    [lo,hi] => hi-lo,
                    _ => panic!("Misshapen windows")
                }
            }) {
                Some(&[lo,hi]) => {
                    (lo,hi)
                },
                _ => panic!("No valid splits"),
            }
        };
        self.splits = [(lo,hi)];
        self.find_tiling(lo, hi);
    }

    fn get_changes(&mut self, emu:&Emulator) {
        let get_changes_fn:Symbol<unsafe extern fn(*mut ScrollChange, u32) -> u32> = emu.get_symbol(b"retro_count_scroll_changes").unwrap();
        unsafe {
            self.change_count = get_changes_fn(self.changes.as_mut_ptr(), 0);
            self.changes.resize_with(self.change_count as usize, Default::default);
            get_changes_fn(self.changes.as_mut_ptr(), self.change_count);
        }
    }
    pub fn dump_tiles(&self, root:&Path) {
        let mut buf = vec![0_u8;8*8*3];
        for (ti,tile) in self.tiles.iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img = ImageBuffer::<Rgb<u8>, _>::from_raw(8, 8, &buf[..]).expect("Couldn't create image buffer");
            img.save(root.join(format!("t{:}.png", ti))).unwrap();
        }
    }

}
