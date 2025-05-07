use crate::framebuffer::Framebuffer;
use crate::metaroom::{Merges, Metaroom, MetaroomID};
use crate::ringbuffer::RingBuffer;
use crate::room::Room;
use crate::screen::Screen;
use crate::sprites::{self, SPRITE_COUNT, SpriteBlob, SpriteData, SpriteTrack};
use crate::tile::{TILE_SIZE, TileDB, TileGfx, TileGfxId};
use crate::time::Timers;
use crate::{Rect, Time};
use image::{ImageBuffer, Rgb};
use retro_rs::{Buttons, Emulator, Symbol};
use std::path::Path;
mod scrolling;
use scrolling::{ScrollChange, ScrollLatch};
mod splits;
use splits::Split;
mod matching;

use crossbeam::channel::{Receiver, Sender, unbounded};
use rayon::{prelude::*, spawn_fifo};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicUsize, Ordering},
};

const DO_TEMP_MERGE_CHECKS: bool = false;

static THREADS_WAITING: AtomicUsize = AtomicUsize::new(0);

// Merge room ID into metarooms with given scores
struct DoMerge(MergePhase, usize, Vec<(MetaroomID, (i32, i32), f32)>);

enum MergePhase {
    Intermediate,
    Finalize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Timing {
    FBRead,
    Scroll,
    Track,
    Blob,
    ReadScreen,
    Control,
    Register,
    FinalizeRoom,
    MergeCalc,
    FinishMerge,
}
impl crate::time::TimerID for Timing {}

pub struct MappyState {
    latch: ScrollLatch,
    pub tiles: Arc<RwLock<TileDB>>,
    pub grid_align: (u8, u8),
    pub scroll: (i32, i32),
    pub has_control: bool,
    pub splits: [(Split, Split); 1],
    pub live_sprites: [SpriteData; SPRITE_COUNT],
    pub prev_sprites: [SpriteData; SPRITE_COUNT],
    pub live_tracks: Vec<SpriteTrack>,
    pub dead_tracks: Vec<SpriteTrack>,
    pub live_blobs: Vec<SpriteBlob>,
    pub dead_blobs: Vec<SpriteBlob>,
    pub current_screen: Screen<TileGfxId>,
    last_control_screen: Screen<TileGfxId>,
    fb: Framebuffer,
    state_buffer: Vec<u8>,
    changes: Vec<ScrollChange>,
    change_count: u32,
    pub current_room: Option<Room>,
    pub rooms: Arc<RwLock<Vec<Room>>>,
    pub metarooms: Merges,
    room_merge_tx: Arc<Sender<DoMerge>>,
    room_merge_rx: Receiver<DoMerge>,
    pub now: Time,
    maybe_control: bool,
    maybe_control_change_time: Time,
    pub last_control: Time,
    pub last_controlled_scroll: (i32, i32),
    pub control_duration: usize,
    pub timers: Timers<Timing>,
    // are we currently mapping?
    pub mapping: bool,
    // which rooms were terminated by resets?
    pub resets: Vec<usize>,
    pub button_inputs: RingBuffer<Buttons>,
}

impl MappyState {
    // 45 frames of no control suggests a room change...
    const CONTROL_ROOM_CHANGE_THRESHOLD: usize = 45;
    // and if this many tiles are different (out of 32*30 = 960) and only a small amount of scrolling happened...
    const SCREEN_ROOM_CHANGE_DIFF_MODERATE: f32 = 170.0;
    // or if this many tiles are different regardless of scrolling...
    const SCREEN_ROOM_CHANGE_DIFF_BIG: f32 = 700.0;
    // We are "in" the room this many frames after we regain control.
    // This is meant to help with situations where the room does some fade-in or something and we get spurious tiles
    const CONTROL_ROOM_ENTER_DURATION: usize = 60;

    const CREATE_COST: u32 = 20;
    const DISTANCE_MAX: u32 = 12;
    const DESTROY_COAST: usize = 5;

    const BLOB_THRESHOLD: f32 = 5.0;
    const BLOB_LOOKBACK: usize = 30;

    const BUTTON_HISTORY: usize = 60;

    // This is just an arbitrary value, not sure what a good one is!
    pub const ROOM_MERGE_THRESHOLD: f32 = 16.0;

    #[must_use]
    pub fn new(w: usize, h: usize) -> Self {
        let db = TileDB::new();
        let t0 = db.get_initial_tile();
        let s0 = Screen::new(Rect::new(0, 0, 0, 0), t0);
        let (room_merge_tx, room_merge_rx) = unbounded();
        let room_merge_tx = Arc::new(room_merge_tx);
        MappyState {
            latch: ScrollLatch::default(),
            tiles: Arc::new(RwLock::new(db)),
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
            control_duration: 0,
            last_controlled_scroll: (0, 0),
            live_sprites: [SpriteData::default(); SPRITE_COUNT],
            prev_sprites: [SpriteData::default(); SPRITE_COUNT],
            live_tracks: Vec::with_capacity(SPRITE_COUNT),
            // just for the current room
            dead_tracks: Vec::with_capacity(128),
            live_blobs: vec![],
            dead_blobs: vec![],
            // last_inputs: [Buttons::new(); INPUT_MEM],
            fb: Framebuffer::new(w, h),
            changes: Vec::with_capacity(32000),
            change_count: 0,
            current_screen: s0.clone(),
            last_control_screen: s0,
            current_room: None,
            rooms: Arc::new(RwLock::new(vec![])),
            metarooms: Merges::new(),
            room_merge_rx,
            room_merge_tx,
            timers: Timers::new(),
            mapping: false,
            resets: vec![],
            button_inputs: RingBuffer::new(Buttons::new(), Self::BUTTON_HISTORY),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn handle_reset(&mut self) {
        if let Some(cr) = self.current_room.as_ref() {
            self.resets.push(cr.id);
        }
        self.finalize_current_room(false);
        self.latch = ScrollLatch::default();
        self.grid_align = (0, 0);
        self.scroll = (0, 0);
        self.has_control = false;
        self.splits = [(
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
        )];
        self.now = Time(0);
        self.last_control = Time(0);
        self.maybe_control = false;
        self.maybe_control_change_time = Time(0);
        self.last_controlled_scroll = (0, 0);
        self.live_sprites
            .iter_mut()
            .for_each(|s| *s = SpriteData::default());
        self.prev_sprites
            .iter_mut()
            .for_each(|s| *s = SpriteData::default());
        self.live_tracks.clear();
        self.dead_tracks.clear();
        self.live_blobs.clear();
        self.dead_blobs.clear();
        self.changes.clear();
        self.change_count = 0;
        let s0 = Screen::new(
            Rect::new(0, 0, 0, 0),
            self.tiles.read().unwrap().get_initial_tile(),
        );
        self.current_screen = s0.clone();
        self.last_control_screen = s0;
    }
    // TODO return a "finalized mappy"
    pub fn finish(&mut self) {
        self.finalize_current_room(false);
        self.process_merges();
        while THREADS_WAITING.load(Ordering::SeqCst) != 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            self.process_merges();
        }
    }

    #[allow(clippy::similar_names, clippy::missing_panics_doc)]
    pub fn process_screen(&mut self, emu: &mut Emulator, input: [Buttons; 2]) {
        // Read new data from emulator
        let t = self.timers.timer(Timing::FBRead).start();
        self.fb.read_from(emu);
        t.stop();
        let t = self.timers.timer(Timing::Scroll).start();
        self.get_changes(emu);

        // What can we learn from hardware screen splitting operations?
        if !self.changes.is_empty() || self.splits.is_empty() {
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
            // dbg!(old_align.1, self.grid_align.1, scrolling::find_offset(old_align.1, self.grid_align.1, 240));
            self.scroll = (
                self.scroll.0
                    + i32::from(scrolling::find_offset(old_align.0, self.grid_align.0, 256)),
                self.scroll.1
                    + i32::from(scrolling::find_offset(old_align.1, self.grid_align.1, 240)),
            );
        }
        t.stop();
        let t = self.timers.timer(Timing::ReadScreen).start();
        // Update current screen tile grid;
        // can't do it on moment 0 since we don't have sprites yet
        if self.now.0 > 0 {
            self.read_current_screen(emu);
        }
        t.stop();

        // avatar identification related:
        self.button_inputs.push(input[0]);

        // Do we have control?
        let had_control = self.has_control;
        let last_control_time = self.last_control;
        self.determine_control(emu);
        self.mapping = false;
        let Rect { w: sw, h: sh, .. } = self.current_screen.region;
        if self.has_control {
            let sdiff = scroll_diff(self.scroll, self.last_controlled_scroll);
            if !had_control {
                // println!(
                //     "{:?}: Regained control after {:?}; scrolldiff {:?}",
                //     self.now.0,
                //     self.now.0 - last_control_time.0,
                //     sdiff
                // );
            }
            if self.now.0 - last_control_time.0 > Self::CONTROL_ROOM_CHANGE_THRESHOLD
                || sdiff.0.unsigned_abs() >= (sw * 3) / 4
                || sdiff.1.unsigned_abs() >= (sh * 3) / 4
            {
                let diff = self.current_screen.difference(&self.last_control_screen);
                // if !had_control {
                // println!(
                //     "{:?}: Regained control after {:?}; diff {:?}, scrolldiff {:?}",
                //     self.now.0,
                //     self.now.0 - last_control_time.0,
                //     diff,
                //     scroll_diff(self.scroll, self.last_controlled_scroll)
                // );
                // }
                let moderate_difference = diff > Self::SCREEN_ROOM_CHANGE_DIFF_MODERATE;
                let big_difference = diff > Self::SCREEN_ROOM_CHANGE_DIFF_BIG;
                let (sdx, sdy) = scroll_diff(self.scroll, self.last_controlled_scroll);
                let small_scroll = (sdx != 0 || sdy != 0) && (sdx.abs() < 150 && sdy.abs() < 150);
                if ((moderate_difference && !small_scroll) || big_difference)
                    || self.current_room.is_none()
                {
                    self.finalize_current_room(true);
                }
            }
            if self.control_duration > Self::CONTROL_ROOM_ENTER_DURATION {
                if let Some(current_room) = self.current_room.as_mut() {
                    self.mapping = true;
                    let t = self.timers.timer(Timing::Register).start();
                    current_room
                        .register_screen(&self.current_screen, &mut self.tiles.write().unwrap());
                    t.stop();
                }
            }
        } else if had_control {
            // dbg!("control loss", self.current_screen.region);
            self.last_control_screen.copy_from(&self.current_screen);
        }
        if self.has_control {
            self.control_duration += 1;
        } else {
            self.control_duration = 0;
        }
        if DO_TEMP_MERGE_CHECKS
            && self.current_room.is_some()
            && self.now.0 % 300 == 0
            && THREADS_WAITING.load(Ordering::SeqCst) == 0
        {
            //spawn room merge thing with self.room_merge_tx
            self.kickoff_merge_calc(
                self.current_room.as_ref().unwrap().clone(),
                MergePhase::Intermediate,
            );
        }
        self.process_merges();

        let t = self.timers.timer(Timing::Track).start();
        // Relate current sprites to previous sprites
        self.track_sprites(emu);
        for track in &mut self.live_tracks {
            track.determine_avatar(self.now, &self.button_inputs);
        }

        t.stop();

        let t = self.timers.timer(Timing::Blob).start();
        self.blob_sprites();
        t.stop();

        // Update `now`
        self.now.0 += 1;

        // Read sprite data for next frame
        self.prev_sprites.copy_from_slice(&self.live_sprites);
        sprites::get_sprites(emu, &mut self.live_sprites);
    }
    fn process_merges(&mut self) {
        if !self.room_merge_rx.is_empty() {
            //let mut metarooms = self.metarooms.write().unwrap();
            while let Ok(DoMerge(phase, room_id, metas)) = self.room_merge_rx.try_recv() {
                match phase {
                    MergePhase::Intermediate => {
                        // for (metaroom, posn, cost) in metas {
                        //metarooms[meta].merge_room(room_id, posn, cost);
                        // println!(
                        //     "Temp merge {} with {:?}: {}@{:?}",
                        //     room_id, metaroom, cost, posn
                        // );
                        // println!(
                        //     "RR:{:?}\nMRR:{:?}",
                        //     self.current_room.as_ref().unwrap().region(),
                        //     self.metarooms
                        //         .metaroom(metaroom.0)
                        //         .region(&(*self.rooms.read().unwrap()))
                        // )
                        // }
                    }
                    MergePhase::Finalize => {
                        //let room_meta = self.metarooms.insert(room_id);
                        let t = self.timers.timer(Timing::FinishMerge).start();
                        self.metarooms.merge_new_room(room_id, &metas);
                        t.stop();
                    }
                }
            }
        }
    }
    fn finalize_current_room(&mut self, start_new: bool) {
        // if we have control now and didn't before and the room changed significantly since then...
        let t = self.timers.timer(Timing::FinalizeRoom).start();
        if self.current_room.is_some() {
            let mut old_room = if start_new {
                let id = {
                    let cur = self.current_room.as_ref().unwrap();
                    cur.id + 1
                };
                // println!("Enter room {}", id);
                self.current_room
                    .replace(Room::new(
                        id,
                        &self.current_screen,
                        &mut self.tiles.write().unwrap(),
                    ))
                    .unwrap()
            } else {
                self.current_room.take().unwrap()
                // println!("Room end {}: {:?}", old_room.id, old_room.region());
            };
            old_room = old_room.finalize(self.tiles.read().unwrap().get_initial_change());
            // dbg!(old_room.region());
            self.kickoff_merge_calc(old_room.clone(), MergePhase::Finalize);
            self.rooms.write().unwrap().push(old_room);
        } else if start_new {
            let id = self.rooms.read().unwrap().len();
            // println!("Room refresh {}", id);
            self.current_room.replace(Room::new(
                id,
                &self.current_screen,
                &mut self.tiles.write().unwrap(),
            ));
        }
        t.stop();
    }
    fn kickoff_merge_calc(&self, room: Room, phase: MergePhase) {
        let tiles = Arc::clone(&self.tiles);
        let rooms = Arc::clone(&self.rooms);
        let mrs = self.metarooms.clone();
        let tx = Arc::clone(&self.room_merge_tx);
        let timer = self.timers.timer(Timing::MergeCalc);
        THREADS_WAITING.fetch_add(1, Ordering::SeqCst);
        // TODO only do this if the current room histogram is different from last merge-checked room histogram
        spawn_fifo(move || {
            let timer = timer.start();
            let merges = mrs
                .metarooms()
                .collect::<Vec<_>>()
                .into_par_iter()
                .filter_map(|metaroom| {
                    // TODO make sure room has significant histogram overlap with at least one room in metaroom
                    if let Some((p, c)) = merge_cost(
                        &room,
                        metaroom.id,
                        &metaroom.registrations,
                        &rooms,
                        &tiles,
                        Self::ROOM_MERGE_THRESHOLD,
                    ) {
                        Some((metaroom.id, p, c))
                    } else {
                        None
                    }
                })
                .collect();
            timer.stop();
            tx.send(DoMerge(phase, room.id, merges))
                .expect("Couldn't send merge message");
            THREADS_WAITING.fetch_sub(1, Ordering::SeqCst);
        });
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn read_current_screen(&mut self, emulator: &Emulator) {
        // if a clear sprite is overlapping a tile, then just place that tile
        // overlapping sprite check. See if it's a tile that's already been seen

        // TODO: here, we can call get_layers and read the actual bg data, and also know for each actual sprite if it has actual contents or not.
        // the magic token for "empty" is 191.

        let mut tiles = self.tiles.write().unwrap();
        let region = self.split_region();
        self.current_screen = Screen::new(
            Rect::new(
                (self.scroll.0 + region.x) / (TILE_SIZE as i32),
                (self.scroll.1 + region.y) / (TILE_SIZE as i32),
                region.w / (TILE_SIZE as u32),
                region.h / (TILE_SIZE as u32),
            ),
            tiles.get_initial_tile(),
        );
        let mut new_ts = 0;
        unsafe {
            // safety: bg_sp, bg, fg_sp may not leak out and we can't run the emulator while they're live
            let [_bg_sp, bg, _fg_sp] = Self::get_layers(emulator);
            for y in (region.y..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                for x in (region.x..(region.x + region.w as i32)).step_by(TILE_SIZE) {
                    let tile =
                        TileGfx::read_slice(bg, self.fb.w, self.fb.h, x as usize, y as usize);
                    if !tiles.contains(&tile) {
                        new_ts += 1;
                        // println!("Unaccounted-for tile, {},{} hash {}", (x-region.x)/(TILE_SIZE as i32), (y-region.y)/(TILE_SIZE as i32), tile.perceptual_hash());
                    }
                    self.current_screen.set(
                        tiles.get_tile(tile),
                        (self.scroll.0 + x) / (TILE_SIZE as i32),
                        (self.scroll.1 + y) / (TILE_SIZE as i32),
                    );
                }
            }
        }
        if new_ts > 10 {
            // println!("{:?} new tiles", new_ts);
            MappyState::dump_tiles_single(
                &Path::new("out").join(format!("tiles_{}.png", self.now.0)),
                &tiles,
            );
        }
    }

    fn determine_control(&mut self, emu: &mut Emulator) {
        // should be long enough to fight momentum
        const CONTROL_CHECK_K: usize = 17;
        // should be odd
        const CONTROL_CHECK_INTERVAL: usize = 7;
        if self.now.0 % CONTROL_CHECK_INTERVAL != 0 {
            return;
        }
        let t = self.timers.timer(Timing::Control).start();
        // every A frames...
        // We'll start with the expensive version and later try the cheaper version if that's too slow.
        // Expensive version:
        // Save state S.
        if self.state_buffer.is_empty() {
            self.state_buffer = vec![0; emu.save_size()];
        }
        if !emu.save(&mut self.state_buffer) {
            println!("Failed to save state");
            return;
        }
        // Apply down-left and b input for K frames
        // TODO: in mario 3 on the level select screen simultaneous presses sometimes cause no movement.  Consider random or alternating down and left and b presses?
        let down_left = Buttons::new()
            .down(true)
            .left(true)
            .b(self.now.0 % 2 == 0)
            .a(self.now.0 % 2 == 1);
        for _ in 0..CONTROL_CHECK_K {
            emu.run([down_left, Buttons::default()]);
        }
        // What can we learn from hardware screen splitting operations?
        self.get_changes(emu);
        let latch = self.latch;
        let (dl_splits, _latch) = splits::get_splits(&self.changes, latch);
        // Store positions of all sprites P1
        let mut sprites_dlb = [SpriteData::default(); SPRITE_COUNT];
        sprites::get_sprites(emu, &mut sprites_dlb);
        // Load state S.
        if !emu.load(&self.state_buffer) {
            println!("failed to load state, ss {} vs state size {}", emu.save_size(), self.state_buffer.len());
            return;
        }
        // Apply up-right and a input for K frames
        let up_right = Buttons::new()
            .up(true)
            .right(true)
            .a(self.now.0 % 2 == 0)
            .b(self.now.0 % 2 == 1);
        for _ in 0..CONTROL_CHECK_K {
            emu.run([up_right, Buttons::default()]);
        }
        self.get_changes(emu);
        let latch = self.latch;
        let (ur_splits, _latch) = splits::get_splits(&self.changes, latch);
        // Store positions of all sprites P2
        let mut sprites_ura = [SpriteData::default(); SPRITE_COUNT];
        sprites::get_sprites(emu, &mut sprites_ura);
        // If P1 != P2 or scroll different, we have control; otherwise we do not
        if !(sprites_dlb == sprites_ura) || dl_splits != ur_splits {
            if !self.maybe_control {
                self.maybe_control_change_time = self.now;
            }
            self.maybe_control = true;
        } else {
            self.maybe_control = false;
        }
        self.has_control = self.maybe_control
            && (self.has_control
                || (self.now.0 - self.maybe_control_change_time.0 > CONTROL_CHECK_K));
        // Load state S.
        if !emu.load(&self.state_buffer) {
            println!("failed to load state");
            return;
        }

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

        t.stop();
    }

    // TODO: increase cost if this would alter blobbing?
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn sprite_change_cost(new_s: &SpriteData, old: &SpriteTrack) -> u32 {
        let sd2 = old.current_data();
        new_s.distance(sd2) as u32
            // avast, ye magic numbers
            + (if old.seen_pattern(new_s.pattern_id) {
                0
            } else {
                2
            })
            + (if old.seen_table(new_s.table) { 0 } else { 4 })
            + (if old.seen_attrs(new_s.attrs) { 0 } else { 4 })
            + (if new_s.height() == sd2.height() { 0 } else { 8 })
            + (if new_s.index == sd2.index { 0 } else { 4 })
    }

    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn track_sprites(&mut self, _emu: &Emulator) {
        use matching::{Match, MatchTo, Target, bnb_match};
        // break up the candidates vec into separate vecs with options that overlap on any index
        fn connected_components(candidates: Vec<MatchTo>) -> Vec<Vec<MatchTo>> {
            fn opts_overlap(opts1: &[Target], opts2: &[Target]) -> bool {
                opts1.iter().any(|Target(maybe_old1, _)| {
                    maybe_old1.is_some()
                        && opts2
                            .iter()
                            .any(|Target(maybe_old2, _)| maybe_old2 == maybe_old1)
                })
            }
            fn spider(candidates: &[MatchTo], gis: &mut [usize], ci1: usize) {
                // recursively add any candidate which is in group 0 and which overlaps with any candidate in the group [ci] on options at all to group gis[ci].
                for (ci2, MatchTo(_new, opts)) in candidates.iter().enumerate() {
                    if gis[ci2] != 0 {
                        continue;
                    }
                    if opts_overlap(opts, &candidates[ci1].1) {
                        gis[ci2] = gis[ci1];
                        spider(candidates, gis, ci2);
                    }
                }
            }
            let mut components = Vec::with_capacity(64);
            let mut gis = vec![0; candidates.len()];
            for ci in 0..candidates.len() {
                // if it's already in a group, do nothing
                if gis[ci] != 0 {
                    continue;
                }
                // otherwise, make a new group
                gis[ci] = components.len() + 1;
                components.push(vec![]);
                // and recursively add any candidate which is in group 0 and which overlaps with any candidate in the group [ci] on options at all to group gis[ci].
                spider(&candidates, &mut gis, ci);
            }
            // now everything has been grouped.
            for (ci, c) in candidates.into_iter().enumerate() {
                assert!(gis[ci] > 0);
                // translate group IDs to indices in components and add components to group.
                components[gis[ci] - 1].push(c);
            }
            components
        }
        let now = self.now;
        let dead_tracks = &mut self.dead_tracks;
        let live_blobs = &mut self.live_blobs;
        let mut dead_blob_ids = vec![];
        self.live_tracks.retain(|t| {
            if now.0 - t.last_observation_time().0 > Self::DESTROY_COAST {
                let id = t.id;
                // println!("{:?} kill {:?}",now,id);
                // TODO this clone shouldn't be necessary
                dead_tracks.push(t.clone());
                // mark t as dead in all blobs using t;
                // if the blob is empty kill it
                for b in live_blobs.iter_mut() {
                    b.kill_track(id);
                    if b.is_dead() {
                        // println!("{:?} kill blob {:?}", now, b.id);
                        dead_blob_ids.push(b.id);
                    }
                }
                false
            } else {
                true
            }
        });
        let dead_blobs = &mut self.dead_blobs;
        self.live_blobs.retain(|b| {
            if dead_blob_ids.contains(&b.id) {
                // TODO this clone shouldn't be necessary
                dead_blobs.push(b.clone());
                false
            } else {
                true
            }
        });
        // find minimal matching of sprites
        // local search is okay
        // vec<vec> is worrisome
        let live: Vec<_> = self
            .live_sprites
            .iter()
            .filter(|s| s.is_valid() && !s.is_empty())
            .collect();
        // dbg!("live", &live);
        // a candidate old track for each new track
        let candidates: Vec<_> = live
            .iter()
            .map(|s| {
                let mut options: Vec<_> = self
                    .live_tracks
                    .iter()
                    .enumerate()
                    .filter_map(|(ti, old)| {
                        if (s.distance(old.current_data()) as u32) < Self::DISTANCE_MAX {
                            Some(Target(Some(ti), Self::sprite_change_cost(s, old)))
                        } else {
                            None
                        }
                    })
                    .collect();
                // possible degenerate case: all sprites in same place
                // dbg!("options:",&options);
                if options.len() > 16 {
                    // fall back to identity match if possible or else None
                    options = vec![
                        options
                            .into_iter()
                            .find(|Target(oi, _cost)| *oi == Some(s.index as usize))
                            .unwrap_or(Target(None, Self::CREATE_COST)),
                    ];
                } else {
                    options.insert(0, Target(None, Self::CREATE_COST));
                }
                MatchTo(s.index as usize, options)
            })
            .collect();
        if candidates.is_empty() {
            // no new sprites at all
            return;
        }
        // let _cl = candidates.len();
        // dbg!("candidates",&candidates);
        let groups = connected_components(candidates);
        // println!("Turned {_cl:?} candidates into {:?} CCs of sizes {:?}", groups.len(), groups.iter().map(|g| g.len()).collect::<Vec<_>>());
        // branch and bound should find the global optimum...
        for candidates in groups {
            // would it be better to phrase this as bipartite matching/flow instead?
            let matching = bnb_match(candidates, self.live_tracks.len());
            // println!("Matched with cost {:?}",cost);
            let mut _new_count = 0;
            let mut _matched_count = 0;
            // println!("Go through {:?}", self.now);
            for Match(new, maybe_oldi) in matching {
                match maybe_oldi {
                    None => {
                        // println!("Create new {:?}", new);
                        _new_count += 1;
                        self.live_tracks.push(SpriteTrack::new(
                            self.live_tracks.len() + self.dead_tracks.len(),
                            self.now,
                            self.scroll,
                            self.live_sprites[new],
                        ));
                        // println!("{:?} create {:?}", now, self.live_tracks.last().unwrap().id);
                    }
                    Some(oldi) => {
                        // match
                        // println!("Update {:?} {:?}", oldi, newi);
                        _matched_count += 1;
                        self.live_tracks[oldi].update(
                            self.now,
                            self.scroll,
                            self.live_sprites[new],
                        );
                    }
                }
            }
        }
        // println!("{:?} LTL {:?}", now, self.live_tracks.len());
    }

    #[allow(clippy::too_many_lines)]
    fn blob_sprites(&mut self) {
        // group track IDs together if they...
        //    tend to be touching
        //    tend to move in the same direction
        let now = self.now;
        let mut unassigned_tracks: Vec<_> = (0..self.live_tracks.len()).collect();
        // TODO: remove from unassigned_tracks any tracks that belong to blobs currently?
        let mut assigned_tracks = Vec::with_capacity(self.live_tracks.len());
        unassigned_tracks.retain(|tx| {
            //find the blob t is best for
            if let Some((bi, score)) = self
                .live_blobs
                .iter()
                .enumerate()
                .map(|(bi, b)| {
                    (
                        bi,
                        b.blob_score(
                            &self.live_tracks[*tx],
                            &self.live_tracks,
                            Self::BLOB_LOOKBACK,
                            self.now,
                        ),
                    )
                })
                .min_by(|(_b1, s1), (_b2, s2)| s1.partial_cmp(s2).unwrap())
            {
                if score < Self::BLOB_THRESHOLD {
                    // TODO: is this already part of some other blob?  give some penalty?
                    assigned_tracks.push((*tx, bi));
                    // assign
                    false
                } else {
                    // remain unassigned
                    true
                }
            } else {
                // remain unassigned
                true
            }
        });
        // find all unassigned live tracks; if any belonged to a blob, remove it from the blob
        for &tx in &unassigned_tracks {
            let id = self.live_tracks[tx].id;
            for b in &mut self.live_blobs {
                if b.contains_live_track(id) {
                    // println!("{:?} remove {:?} from {:?}", now, id, b.id);
                    b.forget_track(id);
                }
            }
            assert!(
                !assigned_tracks.iter().any(|(txx, _bx)| *txx == tx),
                "track {tx:?} both assigned and unassigned {now:?}"
            );
        }
        // for all assigned_tracks, push this track onto the blob
        for (tx, bx) in assigned_tracks {
            let bxid = self.live_blobs[bx].id;
            let txid = self.live_tracks[tx].id;
            assert!(
                !unassigned_tracks.contains(&tx),
                "track {txid:?} both assigned to {bxid:?} and unassigned {now:?}"
            );
            // println!("{:?} assign {:?} to {:?}", now, txid, bxid);
            for b in &mut self.live_blobs {
                if b.id != bxid && b.contains_live_track(txid) {
                    b.forget_track(txid);
                }
            }
            self.live_blobs[bx].use_track(txid);
        }

        let mut blobbed = vec![];
        // for all still unassigned tracks, if any pair can be merged create a blob with them and see if any other unassigned tracks could merge in.
        for (txi, &tx) in unassigned_tracks.iter().enumerate() {
            if blobbed.contains(&txi) {
                continue;
            }
            for (tyi, &ty) in unassigned_tracks.iter().enumerate().skip(txi + 1) {
                if blobbed.contains(&tyi) {
                    continue;
                }
                if SpriteBlob::blob_score_pair(
                    &self.live_tracks[tx],
                    &self.live_tracks[ty],
                    Self::BLOB_LOOKBACK,
                    self.now,
                ) < Self::BLOB_THRESHOLD
                {
                    let mut blob = SpriteBlob::new(self.dead_blobs.len() + self.live_blobs.len());
                    blob.use_track(self.live_tracks[tx].id);
                    blob.use_track(self.live_tracks[ty].id);
                    // println!("{:?} create blob {:?} from {:?} {:?}", now, blob.id, self.live_tracks[tx].id, self.live_tracks[ty].id);
                    blobbed.push(txi);
                    blobbed.push(tyi);
                    for (tzi, &tz) in unassigned_tracks.iter().enumerate().skip(tyi + 1) {
                        if blobbed.contains(&tzi) {
                            continue;
                        }
                        if blob.blob_score(
                            &self.live_tracks[tz],
                            &self.live_tracks,
                            Self::BLOB_LOOKBACK,
                            self.now,
                        ) < Self::BLOB_THRESHOLD
                        {
                            blob.use_track(self.live_tracks[tz].id);
                            // println!("{:?} extend blob {:?} with {:?}", now, blob.id, self.live_tracks[tz].id);
                            blobbed.push(tzi);
                        }
                    }
                    self.live_blobs.push(blob);
                }
            }
        }

        // update centroids of all blobs
        for b in &mut self.live_blobs {
            b.update_position(self.now, &self.live_tracks);
        }

        for (b1i, b1) in self.live_blobs.iter().enumerate() {
            for b2 in self.live_blobs.iter().skip(b1i + 1) {
                for t in &b1.live_tracks {
                    assert!(
                        !b2.live_tracks.contains(t),
                        "track {t:?} appears in two blobs {:?} {:?} at {now:?}",
                        b1.id,
                        b2.id,
                    );
                }
            }
        }
    }

    #[must_use]
    pub fn live_track_with_id(&self, id: &sprites::TrackID) -> Option<&sprites::SpriteTrack> {
        self.live_tracks.iter().find(|t| t.id == *id)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[must_use]
    pub fn split_region(&self) -> Rect {
        splits::split_region_for(
            u32::from(self.splits[0].0.scanline),
            u32::from(self.splits[0].1.scanline),
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
            self.change_count = get_changes_fn(std::ptr::null_mut(), 0);
            self.changes
                .resize_with(self.change_count as usize, Default::default);
            get_changes_fn(self.changes.as_mut_ptr(), self.change_count);
        }
    }
    // safety: these slices don't belong to us so we should drop them
    // before the next time FCEU emulation happens
    #[allow(clippy::similar_names)]
    pub(crate) unsafe fn get_layers(emu: &Emulator) -> [&[u8]; 3] {
        unsafe {
            let get_layer_fn: Symbol<unsafe extern "C" fn(i32) -> *const u8> =
                emu.get_symbol(b"retro_layer").unwrap();
            let sz = 256 * 240;
            let sp_bg = get_layer_fn(0);
            let bg = get_layer_fn(1);
            let sp_fg = get_layer_fn(2);
            [
                std::slice::from_raw_parts(sp_bg, sz),
                std::slice::from_raw_parts(bg, sz),
                std::slice::from_raw_parts(sp_fg, sz),
            ]
        }
    }

    #[must_use]
    pub fn metaroom_exits(&self, mr: &Metaroom) -> Vec<MetaroomID> {
        let mut out_to = vec![];
        for (rid, _pos) in &mr.registrations {
            if self.resets.contains(rid) {
                continue;
            }
            if let Some(mr2) = self
                .metarooms
                .metarooms()
                .find(|mri| mri.registrations.iter().any(|(mrrid, _)| *mrrid == rid + 1))
            {
                if !out_to.contains(&mr2.id) {
                    out_to.push(mr2.id);
                }
            }
        }
        out_to
    }
    #[must_use]
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn world_to_tile(&self, wx: i32, wy: i32) -> (i32, i32) {
        (wx / TILE_SIZE as i32, wy / TILE_SIZE as i32)
    }
    #[must_use]
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    pub fn tile_to_world(&self, tx: i32, ty: i32) -> (i32, i32) {
        (tx * TILE_SIZE as i32, ty * TILE_SIZE as i32)
    }
    #[must_use]
    pub fn world_to_screen(&self, wx: i32, wy: i32) -> (i32, i32) {
        (wx - self.scroll.0, wy - self.scroll.1)
    }
    #[must_use]
    pub fn screen_to_world(&self, sx: i32, sy: i32) -> (i32, i32) {
        (sx + self.scroll.0, sy + self.scroll.1)
    }
    #[must_use]
    pub fn screen_to_tile(&self, sx: i32, sy: i32) -> (i32, i32) {
        let (wx, wy) = self.screen_to_world(sx, sy);
        self.world_to_tile(wx, wy)
    }
    #[must_use]
    pub fn tile_to_screen(&self, tx: i32, ty: i32) -> (i32, i32) {
        let (wx, wy) = self.tile_to_world(tx, ty);
        self.world_to_screen(wx, wy)
    }

    /// # Panics
    /// Panics if the room mutex can't be obtained, or if an I/O error takes place
    pub fn dump_map(&self, dotfolder: &Path) {
        use std::collections::BTreeMap;
        use std::fs;
        use tabbycat::attributes::{Shape, image, shape, xlabel};
        use tabbycat::{AttrList, Edge, GraphBuilder, GraphType, Identity, StmtList};
        let rooms = &self.rooms.read().unwrap();
        let gname = "map".to_string();
        let node_image_paths: BTreeMap<usize, String> = self
            .metarooms
            .all_metarooms()
            .map(|mr| (mr.id.0, format!("mr_{}.png", mr.id.0)))
            .collect();
        let node_labels: BTreeMap<usize, String> = self
            .metarooms
            .metarooms()
            .map(|mr| {
                let r = mr.region(rooms);
                (
                    mr.id.0,
                    format!("{},{}<>{},{}\n", r.x, r.y, r.w, r.h)
                        + &mr
                            .registrations
                            .iter()
                            .map(|(ri, pos)| format!("{}@{},{}", ri, pos.0, pos.1))
                            .collect::<Vec<_>>()
                            .join("\n"),
                )
            })
            .collect();
        let mut all_stmts = StmtList::new();
        for mr in self.metarooms.all_metarooms() {
            self.dump_metaroom(
                mr,
                &dotfolder.join(Path::new(&node_image_paths[&mr.id.0].clone())),
            );
        }
        for mr in self.metarooms.metarooms() {
            let mut stmts = StmtList::new();
            let mr_ident = Identity::from(mr.id.0);
            let mut attrs = AttrList::new()
                .add_pair(xlabel(&node_labels[&mr.id.0]))
                .add_pair(image(&node_image_paths[&mr.id.0]));
            if mr
                .registrations
                .iter()
                .any(|(rid, _pos)| *rid == 0 || self.resets.contains(rid))
            {
                attrs = attrs.add_pair(shape(Shape::Box));
            } else {
                attrs = attrs.add_pair(shape(Shape::Plain));
            }
            stmts = stmts.add_node(mr_ident.clone(), None, Some(attrs));
            for mr2_id in self.metaroom_exits(mr) {
                stmts = stmts.add_edge(
                    Edge::head_node(mr_ident.clone(), None)
                        .arrow_to_node(Identity::from(mr2_id.0), None),
                );
            }
            all_stmts = all_stmts.extend(stmts);
        }
        let graph = GraphBuilder::default()
            .graph_type(GraphType::DiGraph)
            .strict(false)
            .id(Identity::id(&gname).unwrap())
            .stmts(all_stmts)
            .build()
            .unwrap();
        fs::write(dotfolder.join(Path::new("graph.dot")), graph.to_string()).unwrap();
    }
    /// # Panics
    /// May panic if the tile index mutex is poisoned, or if there's an I/O error
    #[allow(clippy::cast_possible_truncation)]
    pub fn dump_tiles(&self, root: &Path) {
        let mut buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in self.tiles.read().unwrap().gfx_iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, &buf[..])
                    .expect("Couldn't create image buffer");
            img.save(root.join(format!("t{ti:}.png"))).unwrap();
        }
    }
    /// # Panics
    /// May panic if the tile index mutex is poisoned, or if there's an I/O error
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn dump_tiles_single(where_to: &Path, tiles: &TileDB) {
        let all_gfx: Vec<_> = tiles.gfx_iter().collect();
        let colrows = (all_gfx.len() as f32).sqrt().ceil() as usize;
        let mut t_buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        let mut buf = vec![0_u8; colrows * colrows * TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in all_gfx.into_iter().enumerate() {
            let row = ti / colrows;
            let col = ti % colrows;
            tile.write_rgb888(&mut t_buf);
            for trow in 0..TILE_SIZE {
                let image_step = TILE_SIZE * 3;
                let image_pitch = colrows * image_step;
                let image_row_start = (row * TILE_SIZE + trow) * image_pitch + col * image_step;
                let image_row_end = (row * TILE_SIZE + trow) * image_pitch + (col + 1) * image_step;
                let tile_row_start = trow * TILE_SIZE * 3;
                let tile_row_end = (trow + 1) * TILE_SIZE * 3;
                assert_eq!(
                    image_row_end - image_row_start,
                    tile_row_end - tile_row_start
                );
                assert_eq!(tile_row_end - tile_row_start, TILE_SIZE * 3);
                for tcolor in 0..TILE_SIZE * 3 {
                    assert_eq!(buf[image_row_start + tcolor], 0);
                }
                buf[image_row_start..image_row_end]
                    .copy_from_slice(&t_buf[tile_row_start..tile_row_end]);
            }
        }
        let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(
            colrows as u32 * TILE_SIZE as u32,
            colrows as u32 * TILE_SIZE as u32,
            &buf[..],
        )
        .expect("Couldn't create image buffer");
        img.save(where_to).unwrap();
    }

    /// # Panics
    /// May panic if the tile index mutex is poisoned, or if there's an I/O error
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn dump_room(&self, room: &Room, at: (u32, u32), tiles_wide: u32, buf: &mut [u8]) {
        let region = room.region();
        let tiles = self.tiles.read().unwrap();
        for y in region.y..(region.y + region.h as i32) {
            for x in region.x..(region.x + region.w as i32) {
                let tile = room.get(x, y);
                let tile_change_data_db =
                    tiles.get_change_by_id(tile.unwrap_or_else(|| tiles.get_initial_change()));
                let to_tile_gfx_id = tile_change_data_db.unwrap().to;
                let corresponding_tile_gfx = tiles.get_tile_by_id(to_tile_gfx_id);
                corresponding_tile_gfx.unwrap().write_rgb888_at(
                    ((x + at.0 as i32 - region.x) * (TILE_SIZE as i32)) as usize,
                    ((y + at.1 as i32 - region.y) * (TILE_SIZE as i32)) as usize,
                    buf,
                    tiles_wide as usize * TILE_SIZE,
                );
            }
        }
    }

    /// # Panics
    /// May panic if the tile index mutex is poisoned, or if there's an I/O error
    #[allow(clippy::cast_possible_truncation)]
    pub fn dump_current_room(&self, path: &Path) {
        let Some(room) = self.current_room.as_ref() else {
            return;
        };
        let region = room.region();
        let mut buf =
            vec![0_u8; TILE_SIZE * (region.w as usize) * TILE_SIZE * (region.h as usize) * 3];
        self.dump_room(room, (0, 0), region.w, &mut buf);
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(
            region.w * TILE_SIZE as u32,
            region.h * TILE_SIZE as u32,
            &buf[..],
        )
        .expect("Couldn't create image buffer");
        img.save(path).unwrap();
    }

    /// # Panics
    /// May panic if a mutex is poisoned or if there's an I/O error
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn dump_metaroom(&self, mr: &Metaroom, path: &Path) {
        // need to dump every room into the same image.
        // so, first get net region of metaroom and build the image buffer.
        // then offset every reg so that the toppiest leftiest reg is at 0,0.
        let Ok(rooms) = self.rooms.read() else {
            return;
        };
        let region = mr.region(&rooms);
        let mut buf =
            vec![0_u8; TILE_SIZE * (region.w as usize) * TILE_SIZE * (region.h as usize) * 3];
        for (room_i, pos) in &mr.registrations {
            assert!(pos.0 - region.x >= 0);
            assert!(pos.1 - region.y >= 0);
            let new_pos = ((pos.0 - region.x) as u32, (pos.1 - region.y) as u32);
            self.dump_room(&rooms[*room_i], new_pos, region.w, &mut buf);
        }
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(
            region.w * TILE_SIZE as u32,
            region.h * TILE_SIZE as u32,
            &buf[..],
        )
        .expect("Couldn't create image buffer");
        img.save(path).unwrap();
    }
}

/// # Panics
/// May panic if a mutex is poisoned
#[allow(clippy::similar_names, clippy::cast_possible_wrap)]
pub fn merge_cost(
    room: &Room,
    _metaroom_id: MetaroomID,
    metaroom: &[(usize, (i32, i32))],
    rooms: &RwLock<Vec<Room>>,
    tiles: &RwLock<TileDB>,
    mut threshold: f32,
) -> Option<((i32, i32), f32)> {
    let mut best = None;
    let ar = room.region();
    let br = {
        let rooms = rooms.read().unwrap();
        let (rid, (x, y)) = metaroom[0];
        let mut rect = Rect {
            x,
            y,
            ..rooms[rid].region()
        };
        for &(rid, (x, y)) in metaroom.iter().skip(1) {
            rect = rect.union(&Rect {
                x,
                y,
                ..rooms[rid].region()
            });
        }
        rect
    };

    let overlap_req = 300.min(ar.w * ar.h / 2).min(br.w * br.h / 2);

    let left = br.x - ar.w as i32;
    let right = br.x + br.w as i32;
    let top = br.y - ar.h as i32;
    let bot = br.y + br.h as i32;
    // dbg!(room.id, metaroom_id, ar, br, left, right, top, bot);
    let rooms = rooms.read().unwrap();
    let tiles = tiles.read().unwrap();
    let initial = tiles.get_initial_change();
    for yo in top..bot {
        'reg: for xo in left..right {
            // put top left of room at x,y and match
            let mut cost = 0.0;
            let mut comparisons = 0;
            // for each tile of the merged room, find the least costly
            // way to match this tile against the correspond tile of
            // any example in the room
            for ry in 0..(ar.h as i32) {
                for rx in 0..(ar.w as i32) {
                    let ax = ar.x + rx;
                    let ay = ar.y + ry;
                    // let bx = br.x + xo + rx;
                    // let by = br.y + yo + ry;
                    let screen1 = room.get_screen_for(ax, ay);
                    if screen1.is_none() {
                        continue;
                    }
                    let room_tile = room.screens[screen1.unwrap()][(ax, ay)];
                    if initial == room_tile {
                        continue;
                    }
                    let mut best_tile_cost = None;
                    for &(room_id, (rxo, ryo)) in metaroom {
                        let room_b = &rooms[room_id];
                        let s2x = rxo + rx + xo;
                        let s2y = ryo + ry + yo;
                        let screen2 = room_b.get_screen_for(s2x, s2y);
                        if screen2.is_none() {
                            continue;
                        }
                        let room_b_tile = room_b.screens[screen2.unwrap()][(s2x, s2y)];
                        // Not really an observation!
                        if !room_b.region().contains(s2x, s2y) {
                            continue;
                        }
                        if initial == room_b_tile {
                            continue;
                        }

                        let tc = tiles.change_cost(room_tile, room_b_tile);
                        // if room.id == 2 && metaroom_id.0 == 0 && xo == 0 && yo == 0 {
                        // dbg!(tc, ax, ay, s2x, s2y, room_id, room_tile, room_b_tile, tiles.get_change_by_id(room_tile), tiles.get_change_by_id(room_b_tile));
                        // }
                        // if room.id == 9 && room_id == 8 {
                        // println!("Compare {:?},{:?} : {:?},{:?} : {:?},{:?} :: {:?}",rx,ry,ax,ay,s2x,s2y,room_b.region());
                        // }
                        // if metaroom_id.0 == 20 && room.id == 39 {
                        // println!("rt {:?}, rbt {:?}, xy {:?}, tc {:?}, best {:?}", room_tile, room_b_tile, (rx,ry), tc, best_tile_cost);
                        // }
                        if tc < best_tile_cost.unwrap_or(f32::MAX) {
                            best_tile_cost = Some(tc);
                        }
                    }
                    if let Some(best_cost) = best_tile_cost {
                        comparisons += 1;
                        cost += best_cost;
                    }
                    if cost >= threshold {
                        // if room.id == 2 && metaroom_id.0 == 0 && xo == 0 && yo == 0 {
                        // dbg!("R2B", room.id, comparisons, overlap_req, cost, threshold);
                        // panic!("done");
                        // }
                        continue 'reg;
                    }
                }
            }
            // if xo == 0 && yo == 0 && room.id == 2 && metaroom_id.0 == 0 {
            // dbg!("R2A", room.id, comparisons, overlap_req, cost, threshold);
            // }
            // if room.id == 39 && metaroom_id.0 == 20 {
            // MappyState::dump_tiles_single(Path::new("out"), &tiles);
            // dbg!(room.id,metaroom_id,xo,yo,comparisons,cost);
            // panic!("done");
            // }
            // if room.id == 8 {
            // dbg!(room.id,xo,yo,comparisons,cost);
            // }
            if cost < threshold && comparisons > overlap_req {
                // dbg!(room.id, comparisons, cost, (xo, yo));
                // assert!(room.id != 1);
                threshold = cost;
                best = Some(((xo, yo), cost));
                if cost == 0.0 {
                    return best;
                }
            }
        }
    }
    // dbg!(self.id, best);
    best
    // for each registration of room.region() onto full, calculate difference across the rooms I have (going a row within each existing room at a time seems good, think about cache effects).  we want to take the best difference and throw away ones that get too bad.  One possibility is to go a row (or a room already in the metaroom, or a room/row combo) at a time and put that into a bnb kind of framework... since we want to find the best one.
    // min_by might work...? but it calculates everything.  I'd like to filter_map and then min_by maybe, or have the min_by sometimes choose to dump in the threshold value + 1.0

    // go through full, registering room at different offsets; bailing out each difference calculation once it gets too big (check every row or col or something).
    // see how room's set of seen changes intersects with each r in self.rooms's... if empty skip it
    // get cost of registering room onto it at best posn
    //   (this is the weighted average of cost of registering at posn wrt all other rooms in the metaroom)
    //      cost of registering ra in rb at posn is just existing room difference but with rects aligned appropriately and out of bounds spots ignored (also maybe taking change cycles into account)
    //   bail out if cost exceeds ROOM_MERGE_THRESHOLD
}
fn scroll_diff((x0, y0): (i32, i32), (x1, y1): (i32, i32)) -> (i32, i32) {
    (x1 - x0, y1 - y0)
}
