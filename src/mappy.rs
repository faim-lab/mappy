use crate::framebuffer::Framebuffer;
// use crate::metaroom::Metaroom;
use crate::metaroom::{Merges, Metaroom, MetaroomID};
use crate::room::Room;
use crate::screen::Screen;
use crate::sprites::{self, SpriteBlob, SpriteData, SpriteTrack, SPRITE_COUNT};
use crate::tile::{TileDB, TileGfx, TileGfxId, TILE_SIZE};
use crate::time::Timers;
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

use crossbeam::channel::{unbounded, Receiver, Sender};
use rayon::{prelude::*, spawn_fifo};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
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
    pub live_tracks: Vec<SpriteTrack>,
    dead_tracks: Vec<SpriteTrack>,
    pub live_blobs: Vec<SpriteBlob>,
    dead_blobs: Vec<SpriteBlob>,
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
    pub timers: Timers<Timing>,
    // which rooms were terminated by resets?
    pub resets: Vec<usize>,
}

impl MappyState {
    // 45 frames of no control
    const CONTROL_ROOM_CHANGE_THRESHOLD: usize = 45;
    // 400 tiles are different (out of 32*30 = 960)
    const SCREEN_ROOM_CHANGE_DIFF: f32 = 400.0;

    const BLOB_THRESHOLD: f32 = 1.0;
    const BLOB_LOOKBACK: usize = 30;

    // This is just an arbitrary value, not sure what a good one is!
    pub const ROOM_MERGE_THRESHOLD: f32 = 16.0;

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

            last_controlled_scroll: (0, 0),
            live_sprites: [SpriteData::default(); SPRITE_COUNT],
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
            resets: vec![],
        }
    }

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
        self.live_tracks.clear();
        self.dead_tracks.clear();
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

    pub fn process_screen(&mut self, emu: &mut Emulator) {
        // Read new data from emulator
        let t = self.timers.timer(Timing::FBRead).start();
        self.fb.read_from(&emu);
        t.stop();
        let t = self.timers.timer(Timing::Scroll).start();
        self.get_changes(&emu);

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
        t.stop();
        let t = self.timers.timer(Timing::ReadScreen).start();
        // Update current screen tile grid
        self.read_current_screen();
        t.stop();

        let t = self.timers.timer(Timing::Track).start();
        sprites::get_sprites(&emu, &mut self.live_sprites);
        // Relate current sprites to previous sprites
        self.track_sprites();
        t.stop();

        let t = self.timers.timer(Timing::Blob).start();
        self.blob_sprites();
        t.stop();

        // Do we have control?
        let had_control = self.has_control;
        let last_control_time = self.last_control;
        self.determine_control(emu);
        if self.has_control {
            if (self.now.0 - last_control_time.0 > Self::CONTROL_ROOM_CHANGE_THRESHOLD
                && self.current_screen.difference(&self.last_control_screen)
                    > Self::SCREEN_ROOM_CHANGE_DIFF)
                || self.current_room.is_none()
            {
                self.finalize_current_room(true);
            } else {
                let t = self.timers.timer(Timing::Register).start();
                self.current_room
                    .as_mut()
                    .unwrap()
                    .register_screen(&self.current_screen, &mut self.tiles.write().unwrap());
                t.stop();
            }
        } else if had_control {
            // dbg!("control loss", self.current_screen.region);
            self.last_control_screen.copy_from(&self.current_screen);
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
        // Update `now`
        self.now.0 += 1;
    }
    fn process_merges(&mut self) {
        if !self.room_merge_rx.is_empty() {
            //let mut metarooms = self.metarooms.write().unwrap();
            while let Ok(DoMerge(phase, room_id, metas)) = self.room_merge_rx.try_recv() {
                match phase {
                    MergePhase::Intermediate => {
                        for (metaroom, posn, cost) in metas {
                            //metarooms[meta].merge_room(room_id, posn, cost);
                            println!(
                                "Temp merge {} with {:?}: {}@{:?}",
                                room_id, metaroom, cost, posn
                            );
                            // println!(
                            //     "RR:{:?}\nMRR:{:?}",
                            //     self.current_room.as_ref().unwrap().region(),
                            //     self.metarooms
                            //         .metaroom(metaroom.0)
                            //         .region(&(*self.rooms.read().unwrap()))
                            // )
                        }
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
                println!("Enter room {}", id);
                self.current_room
                    .replace(Room::new(
                        id,
                        &self.current_screen,
                        &mut self.tiles.write().unwrap(),
                    ))
                    .unwrap()
            } else {
                let old_room = self.current_room.take().unwrap();
                println!("Room end {}: {:?}", old_room.id, old_room.region());
                old_room
            };
            old_room = old_room.finalize(self.tiles.read().unwrap().get_initial_change());
            dbg!(old_room.region());
            self.kickoff_merge_calc(old_room.clone(), MergePhase::Finalize);
            self.rooms.write().unwrap().push(old_room);
        } else if start_new {
            let id = self.rooms.read().unwrap().len();
            println!("Room refresh {}", id);
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
                // .into_iter()
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

    fn read_current_screen(&mut self) {
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
        for y in (region.y..(region.y + region.h as i32)).step_by(TILE_SIZE) {
            for x in (region.x..(region.x + region.w as i32)).step_by(TILE_SIZE) {
                let tile = TileGfx::read(&self.fb, x as usize, y as usize);
                if !tiles.contains(&tile) && sprites::overlapping_sprite(
                    x as usize,
                    y as usize,
                    TILE_SIZE,
                    TILE_SIZE,
                    &self.live_sprites,
                ) {
                    // Just leave the empty one there
                    continue;
                }
                // if !(self.tiles.contains(&tile)) {
                // println!("Unaccounted-for tile, {},{} hash {}", (x-region.x)/(TILE_SIZE as i32), (y-region.y)/(TILE_SIZE as i32), tile.perceptual_hash());
                // }
                self.current_screen.set(
                    tiles.get_tile(tile),
                    (self.scroll.0 + x) / (TILE_SIZE as i32),
                    (self.scroll.1 + y) / (TILE_SIZE as i32),
                );
            }
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
        emu.save(&mut self.state_buffer);
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
        self.get_changes(&emu);
        let latch = self.latch;
        let (dl_splits, _latch) = splits::get_splits(&self.changes, latch);
        // Store positions of all sprites P1
        let mut sprites_dlb = [SpriteData::default(); SPRITE_COUNT];
        sprites::get_sprites(emu, &mut sprites_dlb);
        // Load state S.
        emu.load(&self.state_buffer);
        // Apply up-right and a input for K frames
        let up_right = Buttons::new()
            .up(true)
            .right(true)
            .a(self.now.0 % 2 == 0)
            .b(self.now.0 % 2 == 1);
        for _ in 0..CONTROL_CHECK_K {
            emu.run([up_right, Buttons::default()]);
        }
        self.get_changes(&emu);
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

        t.stop();
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
        let live_blobs = &mut self.live_blobs;
        let mut dead_blob_ids = vec![];
        self.live_tracks.retain(|t| {
            if now.0 - t.last_observation_time().0 > Self::DESTROY_COAST {
                let id = t.id;
                // TODO this clone shouldn't be necessary
                dead_tracks.push(t.clone());
                // mark t as dead in all blobs using t;
                // if the blob is empty kill it
                for b in live_blobs.iter_mut() {
                    b.kill_track(id);
                    if b.is_dead() {
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
                        self.live_tracks.len() + self.dead_tracks.len(),
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

    fn blob_sprites(&mut self) {
        // group track IDs together if they...
        //    tend to be touching
        //    tend to move in the same direction
        let mut unassigned_tracks: Vec<_> = (0..self.live_tracks.len()).collect();
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
                        ),
                    )
                })
                .min_by(|(_b1, s1), (_b2, s2)| s1.partial_cmp(s2).unwrap())
            {
                if score < Self::BLOB_THRESHOLD {
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
        for &tx in unassigned_tracks.iter() {
            let id = self.live_tracks[tx].id;
            for b in self.live_blobs.iter_mut() {
                b.forget_track(id);
            }
        }
        // for all assigned_tracks, push this track onto the blob
        for (tx, bx) in assigned_tracks {
            self.live_blobs[bx].use_track(self.live_tracks[tx].id);
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
                ) < Self::BLOB_THRESHOLD
                {
                    let mut blob = SpriteBlob::new(self.dead_blobs.len() + self.live_blobs.len());
                    blob.use_track(self.live_tracks[tx].id);
                    blob.use_track(self.live_tracks[ty].id);
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
                        ) < Self::BLOB_THRESHOLD
                        {
                            blob.use_track(self.live_tracks[tz].id);
                            blobbed.push(tzi);
                        }
                    }
                    self.live_blobs.push(blob);
                }
            }
        }

        // update centroids of all blobs
        for b in self.live_blobs.iter_mut() {
            b.update_position(self.now, &self.live_tracks);
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
    pub fn dump_map(&self, dotfolder: &Path) {
        use std::collections::BTreeMap;
        use std::fs;
        use tabbycat::attributes::*;
        use tabbycat::{AttrList, Edge, GraphBuilder, GraphType, Identity, StmtList};
        let rooms = &self.rooms.read().unwrap();
        let gname = "map".to_string();
        let node_image_paths: BTreeMap<usize, String> = self
            .metarooms
            .metarooms()
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
        for mr in self.metarooms.metarooms() {
            let mut stmts = StmtList::new();
            let mr_ident = Identity::from(mr.id.0);
            self.dump_metaroom(&mr, &dotfolder.join(Path::new(&node_image_paths[&mr.id.0].clone())));
            let mut attrs = AttrList::new()
                .add_pair(xlabel(&node_labels[&mr.id.0]))
                .add_pair(image(&node_image_paths[&mr.id.0]));
            if let Some(_) = mr
                .registrations
                .iter()
                .find(|(rid, _pos)| *rid == 0 || self.resets.contains(rid))
            {
                attrs = attrs.add_pair(shape(Shape::Box));
            } else {
                attrs = attrs.add_pair(shape(Shape::Plain))
            }
            stmts = stmts.add_node(mr_ident.clone(), None, Some(attrs));
            let mut out_to = vec![];
            for (rid, _pos) in mr.registrations.iter() {
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
            for mr2_id in out_to {
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
    pub fn dump_tiles(&self, root: &Path) {
        let mut buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in self.tiles.read().unwrap().gfx_iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, &buf[..])
                    .expect("Couldn't create image buffer");
            img.save(root.join(format!("t{:}.png", ti))).unwrap();
        }
    }
    pub fn dump_tiles_single(root: &Path, tiles:&TileDB) {
        let all_gfx:Vec<_> = tiles.gfx_iter().collect();
        let colrows = (all_gfx.len() as f32).sqrt().ceil() as usize;
        let mut t_buf = vec![0_u8; TILE_SIZE*TILE_SIZE*3];
        let mut buf = vec![0_u8; colrows * colrows * TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in all_gfx.into_iter().enumerate() {
            let row = ti / colrows;
            let col = ti % colrows;
            tile.write_rgb888(&mut t_buf);
            for trow in 0..TILE_SIZE {
                let image_step = TILE_SIZE * 3;
                let image_pitch = colrows * image_step;
                let image_row_start = (row*TILE_SIZE+trow)*image_pitch+col*image_step;
                let image_row_end = (row*TILE_SIZE+trow)*image_pitch+(col+1)*image_step;
                let tile_row_start = trow*TILE_SIZE*3;
                let tile_row_end = (trow+1)*TILE_SIZE*3;
                assert_eq!(image_row_end-image_row_start, tile_row_end-tile_row_start);
                assert_eq!(tile_row_end-tile_row_start, TILE_SIZE * 3);
                for tcolor in 0..TILE_SIZE*3  {
                    assert_eq!(buf[image_row_start+tcolor], 0);
                }
                buf[image_row_start..image_row_end].copy_from_slice(&t_buf[tile_row_start..tile_row_end]);
            }
        }
        let img: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_raw(colrows as u32 * TILE_SIZE as u32, colrows as u32 * TILE_SIZE as u32, &buf[..])
            .expect("Couldn't create image buffer");
        img.save(root.join("tiles.png")).unwrap();
    }

    pub fn dump_room(&self, room: &Room, at: (u32, u32), tiles_wide: u32, buf: &mut [u8]) {
        let region = room.region();
        let tiles = self.tiles.read().unwrap();
        for y in region.y..(region.y + region.h as i32) {
            for x in region.x..(region.x + region.w as i32) {
                let tile = room.get(x, y);
                let tile_change_data_db = tiles.get_change_by_id(tile);
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

    pub fn dump_current_room(&self, path: &Path) {
        let room = self.current_room.as_ref().unwrap();
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

    pub fn dump_metaroom(&self, mr: &Metaroom, path: &Path) {
        // need to dump every room into the same image.
        // so, first get net region of metaroom and build the image buffer.
        // then offset every reg so that the toppiest leftiest reg is at 0,0.
        let rooms = self.rooms.read().unwrap();
        let region = mr.region(&rooms);
        let mut buf =
            vec![0_u8; TILE_SIZE * (region.w as usize) * TILE_SIZE * (region.h as usize) * 3];
        for (room_i, pos) in mr.registrations.iter() {
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

pub fn merge_cost(
    room: &Room,
    metaroom_id: MetaroomID,
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
    dbg!(room.id, ar,br);

    let overlap_req = ((ar.w.min(br.w)/2)*(ar.h.min(br.h)/2)) as usize;

    let left = br.x-ar.w as i32;
    let right = br.x + br.w as i32;
    let top = br.y-ar.h as i32;
    let bot = br.y + br.h as i32;
    // UGH room 51, 39 still not merging into 13/15/17.  why?  find costs and debug.  can the metaroom ID be passed in as a parameter to make this easier?
    //    It looks like there are slight tile misalignments due to the menu scrolling in and out
    let rooms = rooms.read().unwrap();
    let tiles = tiles.read().unwrap();
    for yo in top..bot {
        for xo in left..right {
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
                    let screen1 = room.get_screen_for(ax,ay);
                    if screen1.is_none() { continue; }
                    let room_tile = room.screens[screen1.unwrap()].get(ax, ay);
                    let mut best_tile_cost = None;
                    for &(room_id, (rxo,ryo)) in metaroom.iter() {
                        let room_b = &rooms[room_id];
                        let s2x = rxo + rx + xo;
                        let s2y = ryo + ry + yo;
                        let screen2 = room_b.get_screen_for(s2x, s2y);
                        if screen2.is_none() { continue; }
                        let room_b_tile = room_b.screens[screen2.unwrap()].get(s2x, s2y);
                        let tc = tiles.change_cost(
                            room_tile,
                            room_b_tile,
                        );
                        // if metaroom_id.0 == 20 && room.id == 39 {
                            // println!("rt {:?}, rbt {:?}, xy {:?}, tc {:?}, best {:?}", room_tile, room_b_tile, (rx,ry), tc, best_tile_cost);
                        // }
                        if tc < best_tile_cost.unwrap_or(f32::MAX) {
                            best_tile_cost = Some(tc);
                        }
                    };
                    if best_tile_cost.is_some() {
                        comparisons += 1;
                        cost += best_tile_cost.unwrap();
                    }
                    if cost >= threshold {
                        break;
                    }
                }
            }
            // if room.id == 39 && (room_id == 17 || room_id == 15 || room_id == 13) {
                // dbg!(room.id, room_id, rxo, rxy, r_cost * num_rooms as f32, r_comparisons, cost, threshold);
            // }
            // if room.id == 39 && metaroom_id.0 == 20 {
                // MappyState::dump_tiles_single(Path::new("out"), &tiles);
                // dbg!(room.id,metaroom_id,xo,yo,comparisons,cost);
                // panic!("done");
            // }
            // dbg!(room.id,xo,yo,comparisons,cost);
            if cost < threshold && comparisons > overlap_req {
                // dbg!(room.id,comparisons,cost);
                threshold = cost;
                best = Some(((xo, yo), cost));
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
