use crate::ringbuffer::RingBuffer;
use crate::Time;
use retro_rs::{Buttons, Emulator};
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SpriteData {
    pub index: u8,
    pub x: u8,
    pub y: u8,
    height: u8,
    pub pattern_id: u8,
    pub table: u8,
    pub attrs: u8,
}
#[allow(dead_code)]
impl SpriteData {
    pub fn width(&self) -> u8 {
        8
    }
    pub fn height(&self) -> u8 {
        self.height
    }
    pub fn vflip(&self) -> bool {
        self.attrs & 0b1000_0000 != 0
    }
    pub fn hflip(&self) -> bool {
        self.attrs & 0b0100_0000 != 0
    }
    pub fn bg(&self) -> bool {
        self.attrs & 0b0010_0000 != 0
    }
    pub fn pal(&self) -> u8 {
        4 + (self.attrs & 0b0000_0011)
    }
    pub fn is_valid(&self) -> bool {
        0 < self.y && self.y < 240
    }
    pub fn key(&self) -> u32 {
        u32::from(self.pattern_id) | (u32::from(self.table) << 8)
    }
    pub fn distance(&self, other: &Self) -> f32 {
        let dx = other.x as f32 - self.x as f32;
        let dy = other.y as f32 - self.y as f32;
        (dx * dx + dy * dy).sqrt()
    }
}
const SPRITE_SIZE: usize = 4;
pub const SPRITE_COUNT: usize = 0x100 / SPRITE_SIZE;
pub fn get_sprites(emu: &Emulator, sprites: &mut [SpriteData]) {
    let buf = &emu.system_ram_ref()[0x0200..0x0200 + SPRITE_COUNT * SPRITE_SIZE];
    // let ppuctrl = 0;
    // TODO put me back when the fceumm build goes up to buildbot
    let ppuctrl = emu.memory_ref(0x2000).expect("Couldn't get PPU CTRL bit")[0];
    let sprite_height: u8 = if ((ppuctrl & 0b0010_0000) >> 5) == 1 {
        16
    } else {
        8
    };
    let table_bit = (ppuctrl & 0b0000_1000) >> 3;
    for (i, bs) in buf.chunks_exact(SPRITE_SIZE).enumerate() {
        let [y, pattern_id, attrs, x] = match *bs {
            [y, pattern_id, attrs, x] => [y, pattern_id, attrs, x],
            _ => unreachable!(),
        };
        sprites[i] = SpriteData {
            index: i as u8,
            x,
            y:y.min(254) + 1,
            height: sprite_height,
            pattern_id,
            table: table_bit,
            attrs,
        }
    }
}

// TODO return list of overlapping sprites
pub fn overlapping_sprite(x: usize, y: usize, w: usize, h: usize, sprites: &[SpriteData]) -> bool {
    for s in sprites.iter().filter(|s| s.is_valid()) {
        // TODO avoid if by rolling into filter?
        // a1 < b2
        // a2 < b1
        if x <= s.x as usize + s.width() as usize
            && s.x as usize <= x + w
            && y <= s.y as usize + s.height() as usize
            && s.y as usize <= y + h
        {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct At(pub Time, pub (i32, i32), pub SpriteData);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct TrackID(usize);

#[derive(Clone)]
pub struct SpriteTrack {
    pub id: TrackID,
    pub positions: Vec<At>,
    // TODO measure against vecs or even arrays?
    pub patterns: HashSet<u8>,
    pub tables: HashSet<u8>,
    pub attrs: HashSet<u8>,
    pub horizontal_control_evidence: (i32, i32),
    pub vertical_control_evidence: (i32, i32),
}

impl SpriteTrack {
    pub fn new(id: usize, t: Time, scroll: (i32, i32), sd: SpriteData) -> Self {
        let mut ret = Self {
            id: TrackID(id),
            positions: vec![],
            patterns: HashSet::new(),
            tables: HashSet::new(),
            attrs: HashSet::new(),
            horizontal_control_evidence: (0, 0),
            vertical_control_evidence: (0, 0),
        };
        ret.update(t, scroll, sd);
        ret
    }
    pub fn current_data(&self) -> &SpriteData {
        &self.positions[self.positions.len() - 1].2
    }
    pub fn last_observation_time(&self) -> Time {
        self.positions[self.positions.len() - 1].0
    }
    pub fn update(&mut self, t: Time, scroll: (i32, i32), sd: SpriteData) {
        // TODO handle time properly, dedup if no change
        // TODO TODO what does that mean?
        self.positions.push(At(t, scroll, sd));
        self.patterns.insert(sd.pattern_id);
        self.tables.insert(sd.table);
        self.attrs.insert(sd.attrs);
    }
    pub fn starting_time(&self) -> Time {
        self.positions[0].0
    }
    pub fn starting_point(&self) -> (i32, i32) {
        let At(_, (sx, sy), sd) = &self.positions[0];
        (sx + sd.x as i32, sy + sd.y as i32)
    }
    pub fn current_point(&self) -> (i32, i32) {
        let At(_, (sx, sy), sd) = &self.positions.last().unwrap();
        (sx + sd.x as i32, sy + sd.y as i32)
    }
    pub fn position_at(&self, t: Time) -> Option<&At> {
        self.positions.iter().rev().find(|At(t0, _, _)| t0 < &t)
    }
    pub fn point_at(&self, t: Time) -> Option<(i32, i32)> {
        self.position_at(t)
            .map(|At(_, (sx, sy), sd)| (sx + sd.x as i32, sy + sd.y as i32))
    }
    pub fn seen_pattern(&self, pat: u8) -> bool {
        self.patterns.contains(&pat)
    }
    pub fn seen_table(&self, tab: u8) -> bool {
        self.tables.contains(&tab)
    }
    pub fn seen_attrs(&self, attrs: u8) -> bool {
        self.attrs.contains(&attrs)
    }
    pub fn velocities(&self, times: std::ops::Range<usize>) -> Vec<(i32, i32)> {
        times
            .map(|t| {
                let (t1x, t1y) = self.point_at(Time(t - 1)).unwrap();
                let (t2x, t2y) = self.point_at(Time(t)).unwrap();
                (t2x - t1x, t2y - t1y)
            })
            .collect()
    }
    pub fn sprites(&self, times: std::ops::Range<usize>) -> Vec<(u8, u8, u8)> {
        times
            .map(|t| {
                let sd = self.position_at(Time(t)).unwrap().2;
                (sd.pattern_id, sd.table, sd.attrs)
            })
            .collect()
    }

    // Here, positive and negative hits are incremented based on whether input changes occur at the same time
    // as changes in acceleration. Also, button inputs are dealt with in int.rs and mappy.rs, and there is a
    // visualizer in int.rs (look at avatar_indicator, and press m while running int.rs to see). What I have right
    // now as a whole works somewhat, but has some issues that need soliving. For instance, it's picking up sprites
    // like blocks that Mario breaks (since they accelerate so fast when they're broken, I think).
    pub fn determine_avatar(&mut self, current_time: Time, button_input: &RingBuffer<Buttons>) {
        // See the struct RingBuffer and the field button_inputs in mappy.rs. This is where
        // player inputs are stored, and then they're passed as a parameter into here
        const LOOKBACK: usize = 60;
        assert!(LOOKBACK <= button_input.get_sz());
        if current_time < Time(LOOKBACK + 1) {
            return;
        }
        const THRESHOLD: f32 = 0.1;
        let early = *current_time - LOOKBACK;
        let middle = *current_time - LOOKBACK / 2;
        if early - 1 > *self.starting_time() {
            // if sprite has existed long enough to look back
            let mid = button_input.get(LOOKBACK / 2);
            let mid_prev = button_input.get(LOOKBACK / 2 + 1);
            let before_velocity = self.velocities(early..middle);
            let before_velocity_x = before_velocity.iter().map(|(vx, _)| *vx as f32).mean();
            let before_velocity_y = before_velocity.iter().map(|(_, vy)| *vy as f32).mean();
            let now_velocity = self.velocities(middle..*current_time);
            let now_velocity_x = now_velocity.iter().map(|(vx, _)| *vx as f32).mean();
            let now_velocity_y = now_velocity.iter().map(|(_, vy)| *vy as f32).mean();
            let mid_x = if mid.get_left() {
                -1
            } else if mid.get_right() {
                1
            } else {
                0
            };
            let mid_prev_x = if mid_prev.get_left() {
                -1
            } else if mid_prev.get_right() {
                1
            } else {
                0
            };
            let mid_y = if mid.get_up() {
                -1
            } else if mid.get_down() {
                1
            } else {
                0
            };
            let mid_prev_y = if mid_prev.get_up() {
                -1
            } else if mid_prev.get_down() {
                1
            } else {
                0
            };
            if mid_x > mid_prev_x {
                if now_velocity_x - before_velocity_x >= THRESHOLD {
                    self.horizontal_control_evidence.0 += 1;
                } else {
                    self.horizontal_control_evidence.1 += 1;
                }
            } else if mid_x < mid_prev_x {
                if before_velocity_x - now_velocity_x >= THRESHOLD {
                    self.horizontal_control_evidence.0 += 1;
                } else {
                    self.horizontal_control_evidence.1 += 1;
                }
            } else {
                // questionable, doesn't account for e.g. braking
                // if (before_velocity_x - now_velocity_x).abs() > THRESHOLD {
                //     self.horizontal_control_evidence.1 += 1;
                // }
            }
            if mid_y > mid_prev_y {
                if now_velocity_y - before_velocity_y >= THRESHOLD {
                    self.vertical_control_evidence.0 += 1;
                } else {
                    self.vertical_control_evidence.1 += 1;
                }
            } else if mid_y < mid_prev_y {
                if before_velocity_y - now_velocity_y >= THRESHOLD {
                    self.vertical_control_evidence.0 += 1;
                } else {
                    self.vertical_control_evidence.1 += 1;
                }
            } else {
                // questionable
                // if (before_velocity_y - now_velocity_y).abs() > THRESHOLD {
                // self.vertical_control_evidence.1 += 1;
                // }
            }
            // if mid_x != mid_prev_x && self.positions.last().unwrap().2.index == 14 {
            //     dbg!(current_time, mid_prev_x,mid_x,before_velocity_x,now_velocity_x,self.horizontal_control_evidence);
            // }
        }
    }

    // Return whether the positive and negative hits pass a threshold (which I have as 5)
    pub fn get_is_avatar(&self) -> bool {
        // TODO: use NPMI between input changes and movement changes.
        (self.horizontal_control_evidence.0 > self.horizontal_control_evidence.1)
            || (self.vertical_control_evidence.0 > self.vertical_control_evidence.1)
    }
}

trait IterStats: Iterator {
    fn mean(self) -> f32
    where
        Self: Sized,
        Self::Item: num_traits::Float,
    {
        use num_traits::cast::ToPrimitive;
        let mut count = 0;
        let mut sum = num_traits::identities::zero::<Self::Item>();
        for elt in self.into_iter() {
            count += 1;
            sum = sum + elt;
        }
        sum.to_f32().unwrap() / count.to_f32().unwrap()
    }
}
impl<Iter, Item> IterStats for Iter where Iter: Iterator<Item = Item> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobID(usize);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteBlob {
    pub id: BlobID,
    pub positions: Vec<(Time, i32, i32)>,
    pub live_tracks: Vec<TrackID>,
    pub dead_tracks: Vec<TrackID>,
}

impl SpriteBlob {
    pub fn new(id: usize) -> Self {
        Self {
            id: BlobID(id),
            positions: vec![],
            live_tracks: vec![],
            dead_tracks: vec![],
        }
    }
    pub fn contains_live_track(&self, ti: TrackID) -> bool {
        self.live_tracks.contains(&ti)
    }
    pub fn forget_track(&mut self, ti: TrackID) {
        if let Some(p) = self.live_tracks.iter().position(|&t| t == ti) {
            self.live_tracks.swap_remove(p);
        }
    }
    pub fn kill_track(&mut self, t: TrackID) {
        if let Some(idx) = self.live_tracks.iter().position(|ti| *ti == t) {
            self.live_tracks.swap_remove(idx);
            self.dead_tracks.push(t);
        }
    }
    pub fn is_dead(&self) -> bool {
        self.live_tracks.is_empty()
    }
    pub fn blob_score_pair(_t1: &SpriteTrack, _t2: &SpriteTrack, _lookback: usize) -> f32 {
        // closeness score: 0 if touching over lookback and diff ID, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving
        100.0
    }
    pub fn blob_score(
        &self,
        _t: &SpriteTrack,
        _all_tracks: &[SpriteTrack],
        _lookback: usize,
    ) -> f32 {
        // closeness score: 0 if touching, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving
        // return min blob score of all of self.live_tracks with id != t.id
        100.0
    }
    pub fn use_track(&mut self, t: TrackID) {
        // add to live if not present
        if !self.live_tracks.contains(&t) {
            self.live_tracks.push(t);
        }
    }
    pub fn update_position(&mut self, t: Time, tracks: &[SpriteTrack]) {
        let tl = self.live_tracks.len() as i32;
        self.positions.push(
            self.live_tracks
                .iter()
                .fold((t, 0, 0), |(t, ax, ay), &tid| {
                    let (bx, by) = tracks
                        .iter()
                        .find(|&tk| tk.id == tid)
                        .unwrap()
                        .current_point();
                    (t, ax + bx / tl, ay + by / tl)
                }),
        );
    }
}
