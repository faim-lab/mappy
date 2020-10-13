use crate::framebuffer::Framebuffer;
use crate::room::Room;
use crate::screen::Screen;
use crate::scrolling::*;
use crate::sprites::{self, SpriteData, SpriteTrack, SPRITE_COUNT};
use crate::tile::{TileDB, TileGfx, TileGfxId, TILE_SIZE};
use crate::{Rect, Time};
use image::{ImageBuffer, Rgb};
use libloading::Symbol;
use retro_rs::{Buttons, Emulator};
use std::path::Path;

const INPUT_MEM: usize = 10;

pub struct MappyState {
    latch: ScrollLatch,
    pub tiles: TileDB,
    pub grid_align: (u8, u8),
    pub scroll: (i32, i32),
    pub has_control: bool,
    pub splits: [(Split, Split); 1],
    pub live_sprites: [SpriteData; SPRITE_COUNT],
    pub live_tracks: Vec<SpriteTrack>,
    dead_tracks: Vec<SpriteTrack>,
    // last_inputs: [Buttons; INPUT_MEM],
    pub current_screen: Screen<TileGfxId>,
    fb: Framebuffer,
    changes: Vec<ScrollChange>,
    change_count: u32,
    pub current_room: Room,
    now: Time,
}

impl MappyState {
    pub fn new(w: usize, h: usize) -> Self {
        let mut db = TileDB::new();
        let t0 = db.get_initial_tile();
        let s0 = Screen::new(Rect::new(0, 0, 0, 0), &t0);
        let room = Room::new(0, &s0, &mut db);
        MappyState {
            latch: ScrollLatch::default(),
            tiles: db,
            grid_align: (0, 0),
            scroll: (0, 0),
            has_control: false,
            splits: [(
                Split {
                    scanline: 0,
                    scroll_x: 0,
                    scroll_y: 0,
                },
                Split {
                    scanline: 240,
                    scroll_x: 0,
                    scroll_y: 0,
                },
            )],
            now: Time(0),
            live_sprites: [SpriteData::default(); SPRITE_COUNT],
            live_tracks: Vec::with_capacity(SPRITE_COUNT),
            // just for the current room
            dead_tracks: Vec::with_capacity(128),
            // last_inputs: [Buttons::new(); INPUT_MEM],
            fb: Framebuffer::new(w, h),
            changes: Vec::with_capacity(32000),
            change_count: 0,
            current_screen: s0,
            current_room: room,
        }
    }
    fn find_tiling(&mut self, lo: Split, _hi: Split) {
        self.grid_align = (lo.scroll_x, lo.scroll_y);
    }
    fn get_splits(&mut self) -> Vec<Split> {
        let mut splits = vec![Split {
            scanline: 0,
            scroll_x: 0,
            scroll_y: 0,
        }];
        for &ScrollChange {
            reason,
            scanline,
            value,
        } in self.changes.iter()
        {
            let scanline = if scanline < 240 { scanline } else { 0 };
            // let old_latch = latch;
            match reason {
                ScrollChangeReason::Read2002 => {
                    self.latch = ScrollLatch::clear();
                }
                ScrollChangeReason::Write2005 => {
                    register_split(&mut splits, scanline + 1);
                    let last = splits.len() - 1;
                    match self.latch {
                        ScrollLatch::H => {
                            splits[last].scroll_x = value;
                        }
                        ScrollLatch::V => {
                            splits[last].scroll_y = value;
                        }
                    };
                    self.latch = self.latch.flip();
                }
                ScrollChangeReason::Write2006 => {
                    let scanline = if scanline > 3 { scanline - 3 } else { scanline };
                    register_split(&mut splits, scanline + 1);
                    let last = splits.len() - 1;
                    match self.latch {
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
                    self.latch = self.latch.flip();
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
        splits
    }
    pub fn get_best_effort_splits(&mut self, splits: &mut [Split]) {
        let fb = &self.fb;
        //If we can skim a rectangle bigger than 24px high at the top or the bottom, our split is height - that
        let down_skim_len = skim_rect(&fb, 0, 1);
        let up_skim_len = skim_rect(&fb, 239, -1);
        if down_skim_len >= 24 && down_skim_len < 120 {
            //move the top split lower
            splits[0].scanline = down_skim_len;
        }
        if up_skim_len >= 24 && up_skim_len < 120 {
            //move the bottom split higher
            splits[1].scanline = 240 - up_skim_len;
        }
    }
    pub fn process_screen(&mut self, emu: &Emulator) {
        self.fb.read_from(&emu);
        self.get_changes(&emu);

        sprites::get_sprites(&emu, &mut self.live_sprites);
        let mut splits = self.get_splits();
        let (lo, hi) = {
            match splits.windows(2).max_by_key(|&win| match win {
                [lo, hi] => hi.scanline - lo.scanline,
                _ => panic!("Misshapen windows"),
            }) {
                Some(&[lo, hi]) => (lo, hi),
                _ => panic!("No valid splits"),
            }
        };
        splits = vec![lo, hi];
        if hi.scanline - lo.scanline >= 239 {
            self.get_best_effort_splits(&mut splits);
        }
        let lo = splits[0];
        let hi = splits[1];
        self.splits = [(lo, hi)];
        let old_align = self.grid_align;
        self.find_tiling(lo, hi);
        // update scroll based on grid align change
        self.scroll = (
            self.scroll.0 + find_offset(old_align.0, self.grid_align.0) as i32,
            self.scroll.1 + find_offset(old_align.1, self.grid_align.1) as i32,
        );
        self.track_sprites();
        self.determine_control();
        if self.has_control {
            // Just don't map at all if we don't have control
            let region = self.split_region();
            self.current_screen = Screen::new(
                Rect::new(
                    (self.scroll.0 + region.x) / (TILE_SIZE as i32),
                    (self.scroll.1 + region.y) / (TILE_SIZE as i32),
                    region.w / (TILE_SIZE as u32),
                    region.h / (TILE_SIZE as u32),
                ),
                &self.tiles.get_initial_tile(),
            );
            for y in (region.y..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                for x in (region.x..(region.x + region.w as i32)).step_by(TILE_SIZE) {
                    if sprites::overlapping_sprite(
                        x as usize,
                        y as usize,
                        TILE_SIZE,
                        TILE_SIZE,
                        &self.live_sprites,
                    ) {
                        // Just leave the empty one there
                        continue;
                    }
                    let tile = TileGfx::read(&self.fb, x as usize, y as usize);
                    // if !(self.tiles.contains(&tile)) {
                    // println!("Unaccounted-for tile, {},{} hash {}", (x-region.x)/(TILE_SIZE as i32), (y-region.y)/(TILE_SIZE as i32), tile.perceptual_hash());
                    // }
                    self.current_screen.set(
                        self.tiles.get_tile(tile),
                        (self.scroll.0 + x) / (TILE_SIZE as i32),
                        (self.scroll.1 + y) / (TILE_SIZE as i32),
                    );
                }
            }
            if self.current_room.id == 0 {
                self.current_room = Room::new(1, &self.current_screen, &mut self.tiles);
            } else {
                self.current_room
                    .register_screen(&self.current_screen, &mut self.tiles);
            }
        }
        self.now.0 += 1;
    }

    fn determine_control(&mut self) {
        // every A frames...
        // We'll start with the expensive version and later try the cheaper version if that's too slow.

        // Expensive version:
        // Save state S.
        // Apply down-left input for K frames
        // Store positions of all sprites P1
        // Load state S.
        // Apply up-right input for K frames
        // Store positions of all sprites P2
        // If P1 != P2, we have control; otherwise we do not
        // Load state S.

        // Cheaper version:
        // Look at the history of sprite movement among live tracks
        // Compare to the recent input history of the last B frames
        // Filter out tracks that are moving in the same direction as the inputs
        //   Store the hardware sprite indices and positions used for these tracks in a vec
        //   Alternative:  Flag a track as "controlled" if it usually moves in the direction of input, over time
        // Save state S
        // Move in one x and one y direction /most different/ from the recent input history for C frames
        //   Question: do I need to actually track during these frames?
        // If in this series of new states the sprites of the corresponding indices are mostly moving in one of the directions we picked, we have control
        //   i.e., for each track, consider the movement of any of the the sprite indices used in that track
        //   Look for a majority of sprite indices used in controlled tracks to move with the new input?
        // Otherwise we don't
        // Load state S

        // Cheapest but tricky version:
        // We have to do /some/ speculative execution because of the case where player holds right during moving right between screens in zelda
        // unless we want to say "any sufficiently fast full-frame period of scrolling (i.e. within D frames) OR big sudden change that doesn't revert (within E frames) indicates a transition"
        // but then we only find out we were scrolling /after/ we're done and have to throw away some stuff we've seen in the room, which is doable if rooms track when they observe tile changes but maybe not the easiest thing, and side effects to the tiledb (especially through room fades) may be annoying

        self.has_control = true;
    }

    const CREATE_COST: u32 = 20;
    const DISTANCE_MAX: u32 = 14;
    const DESTROY_COAST: usize = 5;
    // TODO: increase cost if this would alter blobbing?
    fn sprite_change_cost(new_s: &SpriteData, old: &SpriteTrack) -> u32 {
        let sd2 = &old.positions[old.positions.len() - 1].2;
        new_s.distance(sd2) as u32
            + (if sd2.index == new_s.index { 0 } else { 12 })
            + (if old.seen_pattern(new_s.pattern_id) {
                0
            } else {
                4
            })
            + (if old.seen_table(new_s.table) { 0 } else { 4 })
            + (if old.seen_attrs(new_s.attrs) { 0 } else { 4 })
            + (if new_s.height() == sd2.height() { 0 } else { 8 })
    }
    #[allow(clippy::type_complexity)]
    fn greedy_match(
        mut candidates: Vec<(&SpriteData, Vec<(Option<usize>, u32)>)>,
        track_count: usize,
    ) -> (Vec<(SpriteData, Option<usize>)>, u32) {
        // greedy match:
        // pick candidate with least cost match
        // fix it to that match
        // repeat until done
        let mut used_old: Vec<bool> = vec![false; track_count];
        let mut used_new = [false; SPRITE_COUNT];
        let mut net_cost = 0;
        let mut matching: Vec<(SpriteData, Option<usize>)> = Vec::with_capacity(candidates.len());
        candidates
            .iter_mut()
            .for_each(|(_, opts)| opts.sort_unstable_by_key(|tup| tup.1));
        candidates.sort_unstable_by_key(|(_, opts)| opts.len());
        for (new, opts) in candidates.into_iter() {
            let (maybe_oldi, cost) = opts
                .into_iter()
                .find(|(maybe_oldi, _cost)| match maybe_oldi {
                    Some(oldi) => !used_old[*oldi],
                    None => true,
                })
                .expect("Conflict!  Shouldn't be possible!");
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
        let live: Vec<_> = self.live_sprites.iter().filter(|s| s.is_valid()).collect();
        // a candidate old track for each new track
        let candidates: Vec<_> = live
            .iter()
            .map(|s| {
                (
                    *s,
                    std::iter::once((None, Self::CREATE_COST))
                        .chain(self.live_tracks.iter().enumerate().filter_map(|(ti, old)| {
                            if (s.distance(old.current_data()) as u32) < Self::DISTANCE_MAX {
                                Some((Some(ti), Self::sprite_change_cost(s, &old)))
                            } else {
                                None
                            }
                        }))
                        .collect(),
                )
            })
            .collect();
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
                    self.live_tracks
                        .push(SpriteTrack::new(self.now, self.scroll, new));
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
    const SCREEN_SAFE_LEFT: u32 = 8;
    const SCREEN_SAFE_RIGHT: u32 = 8;
    const SCREEN_SAFE_TOP: u32 = 8;
    const SCREEN_SAFE_BOTTOM: u32 = 8;
    pub fn split_region_for(&self, lo: u32, hi: u32, xo: u8, yo: u8) -> Rect {
        let lo = lo.max(Self::SCREEN_SAFE_TOP);
        let hi = hi.min(self.fb.h as u32 - Self::SCREEN_SAFE_BOTTOM);
        let xo = ((TILE_SIZE - (xo as usize % TILE_SIZE)) % TILE_SIZE) as u32;
        let yo = ((TILE_SIZE - (yo as usize % TILE_SIZE)) % TILE_SIZE) as u32;
        let dy = hi - (lo + yo);
        let dy = (dy / (TILE_SIZE as u32)) * (TILE_SIZE as u32);
        let dx = (self.fb.w as u32 - Self::SCREEN_SAFE_RIGHT) - (xo + Self::SCREEN_SAFE_LEFT);
        let dx = (dx / (TILE_SIZE as u32)) * (TILE_SIZE as u32);
        Rect::new(
            Self::SCREEN_SAFE_LEFT as i32 + xo as i32,
            lo as i32 + yo as i32,
            dx,
            dy,
        )
    }

    pub fn split_region(&self) -> Rect {
        self.split_region_for(
            self.splits[0].0.scanline as u32,
            self.splits[0].1.scanline as u32,
            self.grid_align.0,
            self.grid_align.1,
        )
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
        let mut buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in self.tiles.gfx_iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, &buf[..])
                    .expect("Couldn't create image buffer");
            img.save(root.join(format!("t{:}.png", ti))).unwrap();
        }
    }
}

fn find_offset(old: u8, new: u8) -> i16 {
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

fn skim_rect(fb: &Framebuffer, start: i16, dir: i16) -> u8 {
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
