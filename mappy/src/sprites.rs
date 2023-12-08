use crate::ringbuffer::RingBuffer;
use crate::{Rect, Time};
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
    pub mask: [u8; 16], // will be half-empty for 8px high sprites
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
    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|row| *row == 0)
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
    let (fbw, fbh) = emu.framebuffer_size();
    fn get_mask(x: u8, y: u8, w: u8, h: u8, buf: &[u8], fbw: usize, fbh: usize) -> [u8; 16] {
        const PIX_332_EMPTY: u8 = 191;
        let mut mask = [0_u8; 16];
        for (oy, mask_row) in mask.iter_mut().enumerate().take(h as usize) {
            let yi = y as u16 + oy as u16;
            if yi >= fbh as u16 {
                break;
            }
            for ox in 0..w {
                let xi = x as u16 + ox as u16;
                if xi >= fbw as u16 {
                    break;
                }
                let px = if buf[yi as usize * fbw + xi as usize] != PIX_332_EMPTY {
                    1_u8
                } else {
                    0
                };
                *mask_row |= px << ox;
            }
        }
        mask
    }
    let table_bit = (ppuctrl & 0b0000_1000) >> 3;
    unsafe {
        let [bg_sp, _, fg_sp] = super::MappyState::get_layers(emu);
        for (i, bs) in buf.chunks_exact(SPRITE_SIZE).enumerate() {
            let [y, pattern_id, attrs, x] = match *bs {
                [y, pattern_id, attrs, x] => [y, pattern_id, attrs, x],
                _ => unreachable!(),
            };
            let is_bg = attrs & 0b0010_0000 != 0;
            sprites[i] = SpriteData {
                index: i as u8,
                x,
                y: y.min(254) + 1,
                height: sprite_height,
                pattern_id,
                table: table_bit,
                attrs,
                mask: get_mask(
                    x,
                    y.min(254) + 1,
                    8,
                    sprite_height,
                    if is_bg { bg_sp } else { fg_sp },
                    fbw,
                    fbh,
                ),
            }
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

// Time, scroll offset (redundant across sprites honestly), spritedata
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
    pub fn data_at(&self, t: Time) -> Option<SpriteData> {
        self.position_at(t).map(|At(_, _, sd)| sd).copied()
    }
    pub fn position_at(&self, t: Time) -> Option<&At> {
        self.positions.iter().rev().find(|At(t0, _, _)| t0 <= &t)
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
    pub fn world_positions(&self, times: std::ops::Range<usize>) -> Vec<(i32, i32)> {
        times.map(|t| self.point_at(Time(t)).unwrap()).collect()
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
            match mid_prev_x.cmp(&mid_x) {
                std::cmp::Ordering::Less => {
                    if now_velocity_x - before_velocity_x >= THRESHOLD {
                        self.horizontal_control_evidence.0 += 1;
                    } else {
                        self.horizontal_control_evidence.1 += 1;
                    }
                }
                std::cmp::Ordering::Equal => {}
                std::cmp::Ordering::Greater => {
                    if before_velocity_x - now_velocity_x >= THRESHOLD {
                        self.horizontal_control_evidence.0 += 1;
                    } else {
                        self.horizontal_control_evidence.1 += 1;
                    }
                }
            }
            match mid_prev_y.cmp(&mid_y) {
                std::cmp::Ordering::Less => {
                    if now_velocity_y - before_velocity_y >= THRESHOLD {
                        self.vertical_control_evidence.0 += 1;
                    } else {
                        self.vertical_control_evidence.1 += 1;
                    }
                }
                std::cmp::Ordering::Equal => {}
                std::cmp::Ordering::Greater => {
                    if before_velocity_y - now_velocity_y >= THRESHOLD {
                        self.vertical_control_evidence.0 += 1;
                    } else {
                        self.vertical_control_evidence.1 += 1;
                    }
                }
            }
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
        for elt in self {
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
    // A little redundant, maybe better not to store Time in both places
    // Anyway, won't Time be dense?
    pub bounding_boxes: Vec<(Time, Rect)>,
    pub live_tracks: Vec<TrackID>,
    pub dead_tracks: Vec<TrackID>,
}

impl SpriteBlob {
    pub fn new(id: usize) -> Self {
        Self {
            id: BlobID(id),
            positions: vec![],
            bounding_boxes: vec![],
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
    pub fn blob_score_pair(t1: &SpriteTrack, t2: &SpriteTrack, lookback: usize, now: Time) -> f32 {
        // closeness score: 0 if touching over lookback and diff ID, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving

        assert_ne!(t1.id, t2.id);

        if now.0 <= lookback
            || now - lookback <= t1.starting_time()
            || now - lookback <= t2.starting_time()
        {
            return 100.0;
        }
        let range = (now.0 - lookback)..now.0;
        let vels1 = t1.velocities(range.clone());
        let vels2 = t2.velocities(range.clone());
        let moving = 100.0
            * vels1
                .into_iter()
                .zip(vels2.into_iter())
                .map(|((dx1, dy1), (dx2, dy2))| if dx1 == dx2 && dy1 == dy2 { 0.0 } else { 1.0 })
                .mean();
        // TODO use world_positions is fine, refactor
        let closeness = range
            .map(|t| {
                let r1 = {
                    let (x, y) = t1.point_at(Time(t)).unwrap();
                    Rect::new(
                        x,
                        y,
                        t1.data_at(Time(t)).unwrap().width() as u32,
                        t1.data_at(Time(t)).unwrap().height() as u32,
                    )
                };
                let r2 = {
                    let (x, y) = t2.point_at(Time(t)).unwrap();
                    Rect::new(
                        x,
                        y,
                        t2.data_at(Time(t)).unwrap().width() as u32,
                        t2.data_at(Time(t)).unwrap().height() as u32,
                    )
                };
                if r1.expand(1).overlaps(&r2.expand(1)) {
                    0.0
                } else {
                    100.0
                }
            })
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        closeness + moving
    }
    pub fn blob_score(
        &self,
        t: &SpriteTrack,
        all_tracks: &[SpriteTrack],
        lookback: usize,
        now: Time,
    ) -> f32 {
        // return min blob score of all of self.live_tracks with id != t.id
        self.live_tracks
            .iter()
            .map(|&tid| {
                let track = all_tracks.iter().find(|track| track.id == tid).unwrap();
                if track.id == t.id {
                    100.0
                } else {
                    Self::blob_score_pair(track, t, lookback, now)
                }
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(100.0)
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
        let (_, lx, ly) = self.positions.last().unwrap();
        self.bounding_boxes.push(self.live_tracks.iter().fold(
            (t, Rect::new(*lx, *ly, 1, 1)),
            |(t, r), &tid| {
                let track = tracks.iter().find(|&tk| tk.id == tid).unwrap();
                let (px, py) = track.current_point();
                let dat = track.current_data();
                (
                    t,
                    r.union(&Rect::new(px, py, dat.width() as u32, dat.height() as u32)),
                )
            },
        ));
    }
}
