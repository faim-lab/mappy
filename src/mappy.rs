use crate::framebuffer::Framebuffer;
use crate::room::Room;
use crate::screen::Screen;
use crate::sprites::{self, SpriteData, SpriteTrack, SPRITE_COUNT};
use crate::tile::{TileDB, TileGfx, TileGfxId, TILE_SIZE};
use crate::{Rect, Time};
use image::{ImageBuffer, Rgb};
use libloading::Symbol;
use retro_rs::{Buttons, Emulator};
use std::path::Path;
mod scrolling;
use scrolling::*;
mod splits;
use splits::Split;
mod matching;

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
    last_control_screen: Screen<TileGfxId>,
    fb: Framebuffer,
    state_buffer: Vec<u8>,
    changes: Vec<ScrollChange>,
    change_count: u32,
    pub current_room: Room,
    rooms: Vec<Room>,
    pub now: Time,
    maybe_control: bool,
    maybe_control_change_time: Time,
    pub last_control: Time,
    pub last_controlled_scroll: (i32, i32),
}

impl MappyState {
    const CONTROL_ROOM_CHANGE_THRESHOLD: usize = 60;
    const SCREEN_ROOM_CHANGE_DIFF: f32 = 400.0;
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
            state_buffer: Vec::new(),
            last_control: Time(0),
            maybe_control: false,
            maybe_control_change_time: Time(0),

            last_controlled_scroll: (0, 0),
            live_sprites: [SpriteData::default(); SPRITE_COUNT],
            live_tracks: Vec::with_capacity(SPRITE_COUNT),
            // just for the current room
            dead_tracks: Vec::with_capacity(128),
            // last_inputs: [Buttons::new(); INPUT_MEM],
            fb: Framebuffer::new(w, h),
            changes: Vec::with_capacity(32000),
            change_count: 0,
            current_screen: s0.clone(),
            last_control_screen: s0,
            current_room: room,
            rooms: vec![],
        }
    }

    pub fn process_screen(&mut self, emu: &mut Emulator) {
        // Read new data from emulator
        self.fb.read_from(&emu);
        self.get_changes(&emu);
        sprites::get_sprites(&emu, &mut self.live_sprites);

        // What can we learn from hardware screen splitting operations?
        let (lo, hi, latch) = splits::get_main_split(&self.changes, self.latch, &self.fb);
        self.latch = latch;
        self.splits = [(lo, hi)];

        // Update grid alignment and scrolling
        let old_align = self.grid_align;
        self.grid_align = (lo.scroll_x, lo.scroll_y);
        if self.has_control {
            self.last_controlled_scroll = self.scroll;
        }
        // update scroll based on grid align change
        self.scroll = (
            self.scroll.0 + scrolling::find_offset(old_align.0, self.grid_align.0) as i32,
            self.scroll.1 + scrolling::find_offset(old_align.1, self.grid_align.1) as i32,
        );

        // Relate current sprites to previous sprites
        self.track_sprites();

        // Update current screen tile grid
        self.read_current_screen();

        // Do we have control?
        let had_control = self.has_control;
        let last_control_time = self.last_control;
        self.determine_control(emu);
        if self.has_control {
            if self.current_room.id == 0 {
                self.current_room = Room::new(1, &self.current_screen, &mut self.tiles);
            } else if self.now.0 - last_control_time.0 > Self::CONTROL_ROOM_CHANGE_THRESHOLD
                && self.current_screen.difference(&self.last_control_screen)
                    > Self::SCREEN_ROOM_CHANGE_DIFF
            {
                // if we have control now and didn't before and the room changed significantly since then...
                let id = self.current_room.id + 1;
                let old_room = std::mem::replace(
                    &mut self.current_room,
                    Room::new(id, &self.current_screen, &mut self.tiles),
                );
                println!("Room change {}", id);
                self.rooms.push(old_room);
            } else {
                self.current_room
                    .register_screen(&self.current_screen, &mut self.tiles);
            }
        } else if had_control {
            // dbg!("control loss", self.current_screen.region);
            self.last_control_screen.copy_from(&self.current_screen);
        }

        // Update `now`
        self.now.0 += 1;
    }

    fn read_current_screen(&mut self) {
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
    }

    fn determine_control(&mut self, emu: &mut Emulator) {
        if self.now.0 % 7 != 0 { return; }
        // every A frames...
        // We'll start with the expensive version and later try the cheaper version if that's too slow.
        // Expensive version:
        const K: usize = 10;
        // Save state S.
        if self.state_buffer.is_empty() {
            self.state_buffer = vec![0; emu.save_size()];
        }
        emu.save(&mut self.state_buffer);
        // Apply down-left and b input for K frames
        // TODO: in mario 3 on the level select screen simultaneous presses sometimes cause no movement.  Consider random or alternating down and left and b presses?
        let down_left_b = Buttons::new().down(true).left(true).b(true);
        for _ in 0..K {
            emu.run([down_left_b, Buttons::default()]);
        }
        // Store positions of all sprites P1
        let mut sprites_dlb = [SpriteData::default(); SPRITE_COUNT];
        sprites::get_sprites(emu, &mut sprites_dlb);
        // Load state S.
        emu.load(&self.state_buffer);
        // Apply up-right and a input for K frames
        let up_right_a = Buttons::new().up(true).right(true).a(true);
        for _ in 0..K {
            emu.run([up_right_a, Buttons::default()]);
        }
        // Store positions of all sprites P2
        let mut sprites_ura = [SpriteData::default(); SPRITE_COUNT];
        sprites::get_sprites(emu, &mut sprites_ura);
        // If P1 != P2, we have control; otherwise we do not
        if sprites_dlb != sprites_ura {
            if !self.maybe_control {
                self.maybe_control_change_time = self.now;
            }
            self.maybe_control = true;
        } else {
            self.maybe_control = false;
        }
        self.has_control = self.maybe_control
            && (self.has_control || (self.now.0 - self.maybe_control_change_time.0 > K));
        // Load state S.
        emu.load(&self.state_buffer);

        // Cheaper version:
        // Look at the history of sprite movement among live tracks
        // Compare to the recent input history of the last B frames
        // Filter out tracks that are accelerating in the same direction as the inputs
        //   Store the hardware sprite indices and positions used for these tracks in a vec
        //   Alternative:  Flag a track as "controlled" if it usually accelerates in the direction of input, over time
        // Save state S
        // Move in one x and one y direction /most different/ from the recent input history for C frames
        //   Question: do I need to actually track during these frames?
        // If in this series of new states the sprites of the corresponding indices are mostly accelerating in one of the directions we picked, we have control
        //   i.e., for each track, consider the movement of any of the the sprite indices used in that track
        //   Look for a majority of sprite indices used in controlled tracks to move with the new input?
        // Otherwise we don't
        // Load state S

        // Cheapest but tricky version:
        // We have to do /some/ speculative execution because of the case where player holds right during moving right between screens in zelda
        // unless we want to say "any sufficiently fast full-frame period of scrolling (i.e. within D frames) OR big sudden change that doesn't revert (within E frames) indicates a transition"
        // but then we only find out we were scrolling /after/ we're done and have to throw away some stuff we've seen in the room, which is doable if rooms track when they observe tile changes but maybe not the easiest thing, and side effects to the tiledb (especially through room fades) may be annoying
        if self.has_control {
            self.last_control.0 = self.now.0 + 1;
        }
    }

    const CREATE_COST: u32 = 20;
    const DISTANCE_MAX: u32 = 14;
    const DESTROY_COAST: usize = 5;
    // TODO: increase cost if this would alter blobbing?
    fn sprite_change_cost(new_s: &SpriteData, old: &SpriteTrack) -> u32 {
        let sd2 = old.current_data();
        new_s.distance(sd2) as u32
            // questionable
            //+ (if sd2.index == new_s.index { 0 } else { 12 })
            // okay
            + (if old.seen_pattern(new_s.pattern_id) {
                0
            } else {
                4
            })
            + (if old.seen_table(new_s.table) { 0 } else { 4 })
            + (if old.seen_attrs(new_s.attrs) { 0 } else { 4 })
            + (if new_s.height() == sd2.height() { 0 } else { 8 })
    }

    fn track_sprites(&mut self) {
        use matching::{greedy_match, Match, MatchTo, Target};
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
                MatchTo(
                    s.index as usize,
                    std::iter::once(Target(None, Self::CREATE_COST))
                        .chain(self.live_tracks.iter().enumerate().filter_map(|(ti, old)| {
                            if (s.distance(old.current_data()) as u32) < Self::DISTANCE_MAX {
                                Some(Target(Some(ti), Self::sprite_change_cost(s, &old)))
                            } else {
                                None
                            }
                        }))
                        .collect(),
                )
            })
            .collect();
        if candidates.is_empty() {
            // no new sprites at all
            return;
        }
        //branch and bound should quickly find the global optimum? maybe later
        let matching = greedy_match(candidates, self.live_tracks.len());
        // println!("Matched with cost {:?}",cost);
        let mut _new_count = 0;
        let mut _matched_count = 0;
        // println!("Go through {:?}", self.now);
        for Match(new, maybe_oldi) in matching.into_iter() {
            match maybe_oldi {
                None => {
                    // println!("Create new {:?}", new);
                    _new_count += 1;
                    self.live_tracks.push(SpriteTrack::new(
                        self.now,
                        self.scroll,
                        self.live_sprites[new],
                    ));
                }
                Some(oldi) => {
                    // match
                    // println!("Update {:?} {:?}", oldi, newi);
                    _matched_count += 1;
                    self.live_tracks[oldi].update(self.now, self.scroll, self.live_sprites[new]);
                }
            }
        }
    }

    pub fn split_region(&self) -> Rect {
        splits::split_region_for(
            self.splits[0].0.scanline as u32,
            self.splits[0].1.scanline as u32,
            self.grid_align.0,
            self.grid_align.1,
            self.fb.w as u32,
            self.fb.h as u32,
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
