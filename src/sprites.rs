use crate::Time;
use retro_rs::{Buttons, Emulator};
use std::collections::HashSet;
use crate::mappy::RingBuffer;
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
            y,
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
        // a1 < b2
        // a2 < b1
        if x <= s.x as usize + s.width() as usize
            && s.x as usize <= x + w
            // this +1 is because a sprite is drawn on the scanline -after- its y value? I think?
            && y <= s.y as usize + s.height() as usize + 1
            // could be s.y but we'll keep it more generous just to be safe
            && s.y as usize <= y + h + 1
        {
            return true;
        }
    }
    false
}

#[derive(Clone)]
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
    pub positive_hits: i32,
    pub negative_hits: i32,
    pub button_input: RingBuffer<Buttons>,
}

impl SpriteTrack {
    pub fn new(id: usize, t: Time, scroll: (i32, i32), sd: SpriteData) -> Self {
        let mut ret = Self {
            id: TrackID(id),
            positions: vec![],
            patterns: HashSet::new(),
            tables: HashSet::new(),
            attrs: HashSet::new(),
            positive_hits: 0,
            negative_hits: 0,
            button_input: RingBuffer::new(Buttons::new(), 15),
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
    pub fn starting_point(&self) -> (i32, i32) {
        let At(_, (sx, sy), sd) = &self.positions[0];
        (sx + sd.x as i32, sy + sd.y as i32)
    }
    pub fn current_point(&self) -> (i32, i32) {
        let At(_, (sx, sy), sd) = &self.positions.last().unwrap();
        (sx + sd.x as i32, sy + sd.y as i32)
    }
    pub fn point_at(&self, t: Time) -> Option<(i32, i32)> {
        self.positions
            .iter()
            .rev()
            .find(|At(t0, _, _)| t0 < &t)
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

    pub fn determine_avatar(&mut self, current_time: Time, input: Buttons) {
        // let mut button_input = RingBuffer::new(Buttons::new(), 15);
        self.button_input.push(input); // Push next input into ring buf

        if self.button_input.get_sz() == 16 {
            let back_16 = self.button_input.get(self.button_input.get_sz() - 1);
            let back_15 = self.button_input.get(self.button_input.get_sz() - 2);
            if back_16 != back_15 { // if input changed 15 frames ago
                // determine if average acceleration from 30-15 frames ago differs from that 15-0 frames ago:
                let mut velocities: Vec<(i32, i32)> = Vec::new();
                for i in 0..30 {
                    let pos: Option<(i32, i32)>      = self.point_at(Time(current_time.0-i));
                    let pos_prev: Option<(i32, i32)> = self.point_at(Time(current_time.0-i-3));
                    if pos.is_some() && pos_prev.is_some() {
                        let pos_x: i32      = pos.unwrap().0;
                        let pos_y: i32      = pos.unwrap().1;
                        let pos_x_prev: i32 = pos_prev.unwrap().0;
                        let pos_y_prev: i32 = pos_prev.unwrap().1;
                        let x_vel: i32      = pos_x - pos_x_prev;
                        let y_vel: i32      = pos_y - pos_y_prev;
                        
                        velocities.push((x_vel, y_vel));
                    }
                }

                // // experiment, using velocity instead of acceleration to increment positive and negative hits
                // let mut total_vel_x_15: i32 = 0;
                // let mut total_vel_y_15: i32 = 0;
                // for i in 15..30 {
                //     if velocities.get(i).is_some() {
                //         total_vel_x_15 += velocities.get(i).unwrap().0;
                //         total_vel_y_15 += velocities.get(i).unwrap().1;
                //     }
                // }
                // let mut total_vel_x_30: i32 = 0;
                // let mut total_vel_y_30: i32 = 0;
                // for i in 0..15 {
                //     if velocities.get(i).is_some() {
                //         total_vel_x_30 += velocities.get(i).unwrap().0;
                //         total_vel_y_30 += velocities.get(i).unwrap().1;
                //     }
                // }
                // if total_vel_x_30 - total_vel_x_15 != 0 || total_vel_y_30 - total_vel_y_15 != 0 {
                //     println!("Positive Hit");
                //     self.positive_hits += 1;
                // } else {
                //     println!("Negative Hit");
                //     self.negative_hits += 1;
                // }

                let mut accelerations: Vec<(i32, i32)> = Vec::new();
                for i in 0..(velocities.len()) { // <- this is 30
                    if (velocities.get(velocities.len() - i)).is_some() &&
                       (velocities.get(velocities.len() - i - 1)).is_some() {
        
                        let x_vel: i32 = velocities.get(velocities.len() - i).unwrap().0;
                        let y_vel: i32 = velocities.get(velocities.len() - i).unwrap().1;
        
                        let x_vel_prev: i32 = velocities.get(velocities.len() - i - 1).unwrap().0;
                        let y_vel_prev: i32 = velocities.get(velocities.len() - i - 1).unwrap().1;
        
                        let x_accel: i32 = x_vel - x_vel_prev;
                        let y_accel: i32 = y_vel - y_vel_prev;
                        accelerations.push((x_accel, y_accel));
                    }
                }

                // total of accelerations last 15 frames
                let mut total_accel_x_15: i32 = 0;
                let mut total_accel_y_15: i32 = 0;
                for i in 15..30 {
                    if accelerations.get(i).is_some() {
                        total_accel_x_15 += accelerations.get(i).unwrap().0;
                        total_accel_y_15 += accelerations.get(i).unwrap().1;
                    }
                }

                // total of accelerations last 30-15 frames ago               
                let mut total_accel_x_30: i32 = 0;
                let mut total_accel_y_30: i32 = 0;
                for i in 0..15 {
                    if accelerations.get(i).is_some() {
                        total_accel_x_30 += accelerations.get(i).unwrap().0;
                        total_accel_y_30 += accelerations.get(i).unwrap().1;
                    }
                }

                // If average acceleration in 15-0 frames ago differs from that  30-15 ago
                if (total_accel_x_30 - total_accel_x_15).abs() > 1 || (total_accel_y_30 - total_accel_y_15).abs() > 1 {
                    self.positive_hits += 1;
                } else {
                    self.negative_hits += 1;
                }

            }
        }
    }

    pub fn get_is_avatar(&self) -> bool {
        return self.positive_hits - self.negative_hits > 0 // greater than a threshold value
    }
}

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
    pub fn blob_score_pair(t1: &SpriteTrack, t2: &SpriteTrack, lookback: usize) -> f32 {
        // closeness score: 0 if touching over lookback and diff ID, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving
        100.0
    }
    pub fn blob_score(&self, t: &SpriteTrack, all_tracks: &[SpriteTrack], lookback: usize) -> f32 {
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

