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
    pub is_avatar: bool,
}

impl SpriteTrack {
    pub fn new(id: usize, t: Time, scroll: (i32, i32), sd: SpriteData) -> Self {
        let mut ret = Self {
            id: TrackID(id),
            positions: vec![],
            patterns: HashSet::new(),
            tables: HashSet::new(),
            attrs: HashSet::new(),
            is_avatar: false,
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

    pub fn get_is_avatar(&self, input: Buttons, current_time: Time) -> bool {
        let mut button_input = RingBuffer::new(Buttons::new(), 15);
        // Push button_input into a RingBuffer
        button_input.push(input);

        // Counters used to determine most common input in current ring buffer:
        let mut counter_left: i32  = 0;
        let mut counter_right: i32 = 0;
        let mut counter_jump: i32  = 0;

        let mut frequency: i32 = 0;

        for n in 0..(button_input.get_sz()) {
            if button_input.get(n).get_left() {
                counter_left += 1
            }
            if button_input.get(n).get_right() {
                counter_right += 1
            }
            if button_input.get(n).get_a() {
                counter_jump += 1
            }
        }

        let dominant_input: i32 =
        if counter_right < counter_left && counter_jump < counter_left {
            1 // left
        } else if counter_left < counter_right && counter_jump < counter_right {
            2 //right
        } else if counter_left < counter_jump && counter_right < counter_jump {
            3 // jump
        } else {
            0 // no input
        };
        
        // Extracting the position at different points. For position, velocity, and acceleration
        let pos_1: Option<(i32, i32)> = self.point_at(Time(current_time.0-15));  // 15 frames back
        let pos_2: Option<(i32, i32)> = self.point_at(Time(current_time.0-13)); // 13 frames back
        let pos_3: Option<(i32, i32)> = self.point_at(Time(current_time.0-11)); // 11 frames back
        let pos_4: Option<(i32, i32)> = self.point_at(Time(current_time.0-9));  // 9 frames back
        let pos_5: Option<(i32, i32)> = self.point_at(Time(current_time.0-7));  // 7 frames back
        let pos_6: Option<(i32, i32)> = self.point_at(Time(current_time.0-5)); // 5 frames back
        let pos_7: Option<(i32, i32)> = self.point_at(Time(current_time.0-3)); // 3 frames back
        let pos_8: Option<(i32, i32)> = self.point_at(Time(current_time.0-1));  // 1 frames back 

        if pos_1.is_some() && pos_2.is_some() && pos_3.is_some() && pos_4.is_some() &&
           pos_5.is_some() && pos_6.is_some() && pos_7.is_some() && pos_8.is_some() {
            // Unwrap and separate the x and y coordinates
            let pos_1_x: i32 = pos_1.unwrap().0;
            let pos_1_y: i32 = pos_1.unwrap().1;
            let pos_2_x: i32 = pos_2.unwrap().0;
            let pos_2_y: i32 = pos_2.unwrap().1;
            let pos_3_x: i32 = pos_3.unwrap().0;
            let pos_3_y: i32 = pos_3.unwrap().1;
            let pos_4_x: i32 = pos_4.unwrap().0;
            let pos_4_y: i32 = pos_4.unwrap().1;
            let pos_5_x: i32 = pos_5.unwrap().0; // new ones from here
            let pos_5_y: i32 = pos_5.unwrap().1;
            let pos_6_x: i32 = pos_6.unwrap().0;
            let pos_6_y: i32 = pos_6.unwrap().1;
            let pos_7_x: i32 = pos_7.unwrap().0;
            let pos_7_y: i32 = pos_7.unwrap().1;
            let pos_8_x: i32 = pos_8.unwrap().0;
            let pos_8_y: i32 = pos_8.unwrap().1;

            let x_vel_1: i32 = pos_1_x - pos_2_x; // Velocities
            let x_vel_2: i32 = pos_3_x - pos_4_x;
            let x_vel_3: i32 = pos_5_x - pos_6_x;
            let x_vel_4: i32 = pos_7_x - pos_8_x;
            let y_vel_1: i32 = pos_1_y - pos_2_y;
            let y_vel_2: i32 = pos_3_y - pos_4_y;
            let y_vel_3: i32 = pos_5_y - pos_6_y;
            let y_vel_4: i32 = pos_7_y - pos_8_y;

            let x_accel_1: i32 = x_vel_1 - x_vel_2; // Accelerations
            let x_accel_2: i32 = x_vel_3 - x_vel_4;
            let y_accel_1: i32 = y_vel_1 - y_vel_2;
            let y_accel_2: i32 = y_vel_3 - y_vel_4;

            // // what to put in this get parameter is questionable
            // if x_accel_1 > 0 && (button_input.get(10).get_right() || button_input.get(8).get_right()) { 
            //     frequency += 1
            // }
            // if x_accel_1 < 0 && (button_input.get(10).get_left() || button_input.get(8).get_left()) {
            //     frequency += 1
            // }
            // if x_accel_2 > 0 && (button_input.get(3).get_right() || button_input.get(0).get_right()) {
            //     frequency += 1
            // }
            // if x_accel_2 < 0 && (button_input.get(3).get_left() || button_input.get(0).get_left()) {
            //     frequency += 1
            // }

            // Velocities:
            let x_vel_recent: i32 = pos_6_x - pos_4_x;
            let x_vel_prev: i32   = pos_3_x - pos_1_x;
            let y_vel_recent: i32 = pos_6_y - pos_4_y;
            let y_vel_prev: i32   = pos_3_y - pos_1_y;

            // Accelerations:
            let x_accel: i32 = x_vel_recent - x_vel_prev;
            let y_accel: i32 = y_vel_recent - y_vel_prev;

            // if x_vel_recent < 0 && dominant_input == 2 { // determine avatar via velocity
            //     frequency += 1
            // }
            // if x_vel_recent > 0 && dominant_input == 1 {
            //     frequency += 1
            // }
            // if y_vel_recent > 0 && dominant_input == 3 {
            //     frequency += 1
            // }

            if x_accel > 0 && dominant_input == 2 { // determine avatar via acceleration
                frequency += 1
            }
            if x_accel < 0 && dominant_input == 1 {
                frequency += 1
            }
            if y_accel < 0 && dominant_input == 3 {
                frequency += 1
            }

        }
        // println!("{}", frequency);
        return frequency >= 1 // greater than a threshold value
    }
    // NEXT: CLEAN UP MAPPY.RS
}
    // pub fn to_string(&self) -> String {
    //     format!("{:?} {:?}", self.id, self.patterns)
    // }

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

