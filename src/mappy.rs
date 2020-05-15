use crate::framebuffer::Framebuffer;
use crate::scrolling::*;
use crate::sprites::{SpriteData, SpriteTrack, SPRITE_COUNT, self};
use crate::{Rect,Time};
use crate::tile::{TileGfxId, TileDB, TileGfx};
use crate::screen::Screen;
use crate::room::Room;
use image::{ImageBuffer, Rgb};
use itertools::Itertools;
use libloading::Symbol;
use retro_rs::{Emulator,Buttons};
use std::collections::HashSet;
use std::path::Path;

const INPUT_MEM:usize = 10;

pub struct MappyState {
    latch: ScrollLatch,
    pub tiles: TileDB,
    pub grid_align: (u8, u8),
    pub scroll: (i32,i32),
    pub splits: [(u8, u8); 1],
    pub live_sprites: [SpriteData; SPRITE_COUNT],
    pub live_tracks: Vec<SpriteTrack>,
    dead_tracks: Vec<SpriteTrack>,
    // last_inputs: [Buttons; INPUT_MEM],
    pub current_screen: Screen<TileGfxId>,
    fb: Framebuffer,
    changes: Vec<ScrollChange>,
    change_count: u32,
    pub current_room: Room,
    now: Time
}

impl MappyState {
    pub fn new(w: usize, h: usize) -> Self {
        let mut db = TileDB::new();
        let t0 = db.get_initial_tile();
        let s0 = Screen::new(Rect::new(0,0,0,0),&t0);
        let room = Room::new(0,&s0,&mut db);
        MappyState {
            latch: ScrollLatch::default(),
            tiles:db,
            grid_align: (0, 0),
            scroll: (0,0),
            splits: [(0, 240)],
            now:Time(0),
            live_sprites: [SpriteData::default(); SPRITE_COUNT],
            live_tracks: Vec::with_capacity(SPRITE_COUNT),
            // just for the current room
            dead_tracks: Vec::with_capacity(128),
            // last_inputs: [Buttons::new(); INPUT_MEM],
            fb: Framebuffer::new(w, h),
            changes: Vec::with_capacity(32000),
            change_count: 0,
            current_screen: s0,
            current_room: room
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
            self.scroll.0 + find_offset(old_align.0, self.grid_align.0) as i32,
            self.scroll.1 + find_offset(old_align.1, self.grid_align.1) as i32
        );
        let region = self.split_region();
        self.current_screen = Screen::new(Rect::new((self.scroll.0+region.x)/8, (self.scroll.1+region.y)/8, region.w/8, region.h/8), &self.tiles.get_initial_tile());
        for y in (region.y..(region.y+region.h as i32)).step_by(8) {
            for x in (region.x..(region.x+region.w as i32)).step_by(8) {
                if sprites::overlapping_sprite(x as usize, y as usize, &self.live_sprites) {
                    // Just leave the empty one there
                    continue;
                }
                // TODO could we avoid double-reading the framebuffer? We already did it to align the grid...
                let tile = TileGfx::read(&self.fb, x as usize, y as usize);
                if !(self.tiles.contains(&tile)) {
                    println!("Unaccounted-for tile, {},{} hash {}", (x-region.x)/8, (y-region.y)/8, tile.perceptual_hash());
                }
                self.current_screen.set(self.tiles.get_tile(tile), (self.scroll.0+x)/8, (self.scroll.1+y)/8);
            }
        }
        if self.current_room.id == 0 {
            self.current_room = Room::new(1, &self.current_screen, &mut self.tiles);
        } else {
            self.current_room.register_screen(&self.current_screen, &mut self.tiles);
        }
        self.track_sprites();
        self.now.0 += 1;
    }

    const CREATE_COST:u32 = 20;
    const COAST_COST:u32 = 15;
    const DISTANCE_MAX:u32 = 14;
    const DESTROY_COAST:usize = 5;
    // TODO: increase cost if this would alter blobbing?
    fn sprite_change_cost(new_s:&SpriteData, old:&SpriteTrack) -> u32 {
        let sd2 = &old.positions[old.positions.len()-1].2;
        new_s.distance(sd2) as u32 +
            (if sd2.index == new_s.index { 0 } else { 12 }) +
            (if old.seen_pattern(new_s.pattern_id) { 0 } else { 4 }) +
            (if old.seen_table(new_s.table) { 0 } else { 4 }) +
            (if old.seen_attrs(new_s.attrs) { 0 } else { 4 }) +
            (if new_s.height() == sd2.height() { 0 } else { 8 })
    }
    fn greedy_match(mut candidates:Vec<(&SpriteData, Vec<(Option<usize>,u32)>)>, track_count:usize) -> (Vec<(SpriteData, Option<usize>)>, u32) {
        // greedy match:
        // pick candidate with least cost match
        // fix it to that match
        // repeat until done
        let mut used_old:Vec<bool> = vec![false;track_count];
        let mut used_new = [false;SPRITE_COUNT];
        let mut net_cost = 0;
        let mut matching:Vec<(SpriteData, Option<usize>)> = Vec::with_capacity(candidates.len());
        candidates.iter_mut().for_each(|(_,opts)| opts.sort_unstable_by_key(|tup| tup.1));
        candidates.sort_unstable_by_key(|(_,opts)| opts.len());
        for (new, opts) in candidates.into_iter() {
            let (maybe_oldi, cost) = opts.into_iter().find(|(maybe_oldi, _cost)| match maybe_oldi {
                Some(oldi) => !used_old[*oldi],
                None => true
            }).expect("Conflict!  Shouldn't be possible!");
            assert!(!used_new[new.index as usize]);
            used_new[new.index as usize] = true;
            net_cost += cost;
            match maybe_oldi {
                Some(oldi) => {
                    used_old[oldi] = true;
                    matching.push((*new, Some(oldi)));
                }
                None => {
                    matching.push((*new, None));
                }
            }
        }
        // TODO increase net_cost by coast_cost for each old track with no matching new track?
        (matching, net_cost)
    }
    fn track_sprites(&mut self) {
        let now = self.now;
        let dead_tracks = &mut self.dead_tracks;
        self.live_tracks.retain(|t| {
            if now.0 - t.last_observation_time().0 > Self::DESTROY_COAST {
                dead_tracks.push(t.clone());
                false
            } else {
                true
            }
        });

        // find minimal matching of sprites
        // local search is okay
        // vec<vec> is worrisome
        let live:Vec<_> = self.live_sprites
            .iter()
            .filter(|s| s.is_valid())
            .collect();
        // a candidate old track for each new track

        let candidates:Vec<(&SpriteData, Vec<(Option<usize>, u32)>)> =
            live.iter()
            .map(
                |s|
                (*s,
                 std::iter::once((None, Self::CREATE_COST)).chain(
                     self.live_tracks
                         .iter()
                         .enumerate()
                         .filter_map(
                             |(ti,old)|
                             if (s.distance(old.current_data()) as u32) < Self::DISTANCE_MAX {
                                 Some((Some(ti), Self::sprite_change_cost(s, &old)))
                             } else {
                                 None
                             })
                 ).collect())).collect();
        if candidates.is_empty() && self.live_tracks.is_empty() {
            // no old and no new sprites
            return;
        }
        //branch and bound should quickly find the global optimum? maybe later
        let (matching, _cost) = Self::greedy_match(candidates, self.live_tracks.len());
        // println!("Matched with cost {:?}",cost);
        let mut _new_count = 0;
        let mut _matched_count = 0;
        // println!("Go through {:?}", self.now);
        for (new, maybe_oldi) in matching.into_iter() {
            match maybe_oldi {
                None => {
                    println!("Create new {:?}", new.index);
                    _new_count += 1;
                    self.live_tracks.push(SpriteTrack::new(self.now, self.scroll, new));
                }
                Some(oldi) => {
                    // match
                    // println!("Update {:?} {:?}", oldi, newi);
                    _matched_count += 1;
                    self.live_tracks[oldi].update(self.now, self.scroll, new);
                }
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
        for (ti, tile) in self.tiles.gfx_iter().enumerate() {
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
