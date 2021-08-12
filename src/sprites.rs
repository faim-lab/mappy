use crate::Rect;
use crate::Time;
use retro_rs::Emulator;
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

    // a rectangle for the sprites
    pub fn sprite_rect(&self) -> Rect {
        let s_rect = Rect::new(
            self.x as i32,
            self.y as i32,
            self.width() as u32,
            self.height() as u32,
        );
        s_rect
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

// struct holding time, coordinates, and the sprite's data
#[derive(Clone, Debug)]
pub struct At(pub Time, pub (i32, i32), pub SpriteData);

// struct of a Track's ID
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
    // pub total_moves: usize,
    // pub continuous_moves: usize,
}

impl SpriteTrack {
    pub fn new(id: usize, t: Time, scroll: (i32, i32), sd: SpriteData) -> Self {
        let mut ret = Self {
            id: TrackID(id),
            positions: vec![],
            patterns: HashSet::new(),
            tables: HashSet::new(),
            attrs: HashSet::new(),
        };
        ret.update(t, scroll, sd);
        ret
    }
    // TODO add function that just returns the sprite data q?
    pub fn getSpriteData(&self) -> &SpriteData {
        return &self.positions[0].2;
    }
    pub fn current_data(&self) -> &SpriteData {
        &self.positions[self.positions.len() - 1].2
    }
    pub fn last_observation_time(&self) -> Time {
        self.positions[self.positions.len() - 1].0
    }
    pub fn first_observation_time(&self) -> Time {
        self.positions[0].0
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
    pub fn position_at(&self, t: Time) -> Option<(i32, i32)> {
        let At(_, (sx, sy), sd) = &self
            .positions
            .iter()
            .rev()
            .find(|At(time, _, _)| *time <= t)
            .unwrap_or_else(|| panic!("{:?}   {:?}", t, self.positions));
        Some((sx + sd.x as i32, sy + sd.y as i32))
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
    pub fn distance(&self, index1: usize, index2: usize) -> f32 {
        // this function should calculate the distance between two positions
        let At(_, (sx, sy), sd) = &self.positions[index1];
        let At(_, (sx2, sy2), sd2) = &self.positions[index2];
        let (x, x2) = (sx + sd.x as i32, sx2 + sd2.x as i32);
        let (y, y2) = (sy + sd.y as i32, sy2 + sd2.y as i32);

        let dx = x2 - x;
        let dy = y2 - y;

        let dist = (dx * dx + dy * dy) as f32;
        dist.sqrt()
    }
    /// Returns the frequency that a track is world stationary and screen stationary
    ///
    /// # Arguments
    /// None
    ///
    pub fn stationary_ratio(&self) -> (f32, f32) {
        let mut world_count = 0;
        let mut screen_count = 0;
        for window in self.positions.windows(2) {
            let At(time, (sx, sy), sd) = window[0];
            let At(time2, (sx2, sy2), sd2) = window[1];
            let (dispx, dispy) = (
                (sx + sd.x as i32) - (sx2 + sd2.x as i32),
                (sy + sd.y as i32) - (sy2 + sd2.y as i32),
            );
            let world_dist = ((dispx * dispx + dispy * dispy) as f32).sqrt();
            let screen_dist = (((sx - sx2) * (sx - sx2) + (sy * sy2) * (sy * sy2)) as f32).sqrt();
            if world_dist == 0.0 {
                world_count += 1;
            }
            if screen_dist == 0.0 {
                screen_count += 1;
            }
        }

        return (world_count as f32, screen_count as f32);
    }

    /// Returns a boolean pair of whether or not a track is world stationary and screen stationary
    ///
    /// # Arguments
    /// None
    ///
    /// Note that tracks can be both world stationary and screen stationary. The two are not exclusive.
    ///
    pub fn stationary_bool(&self) -> (bool, bool) {
        let world_bool: bool;
        let screen_bool: bool;
        let move_count = (self.positions.len() - 1) as f32; // how many moves there are ex: 3 positions -> 2 moves
        let (world_count, screen_count) = self.stationary_ratio();
        let (world_freq, screen_freq) = (world_count / move_count, screen_count / move_count);

        if world_freq > 0.97 {
            world_bool = true;
        } else {
            world_bool = false;
        }
        if screen_freq > 0.97 {
            screen_bool = true;
        } else {
            screen_bool = false;
        }
        return (world_bool, screen_bool);
    }

    /// Returns the frequency at which a track is continuously moving and discretely moving
    ///
    /// # Arguments
    /// None
    ///
    pub fn moving_ratio(&self) -> (f32, f32) {
        let mut continuous_count = 0;
        let mut world_count = 0;
        let mut screen_count = 0;
        let mut discrete_count = 0;

        for window in self.positions.windows(2) {
            let At(time, (sx, sy), sd) = window[0];
            let At(time2, (sx2, sy2), sd2) = window[1];
            let (dispx, dispy) = (
                (sx + sd.x as i32) - (sx2 + sd2.x as i32),
                (sy + sd.y as i32) - (sy2 + sd2.y as i32),
            );
            let world_dist = ((dispx * dispx + dispy * dispy) as f32).sqrt();
            let screen_dist = (((sx - sx2) * (sx - sx2) + (sy * sy2) * (sy * sy2)) as f32).sqrt();

            if world_dist > 0.0 && world_dist <= 8.0 {
                continuous_count += 1;
            }
            if world_dist > 8.0 || screen_dist > 8.0 {
                discrete_count += 1;
            }
            if world_dist > 0.0 {
                world_count += 1;
            }
            if screen_dist > 0.0 {
                screen_count += 1;
            }
        }
        return (continuous_count as f32, discrete_count as f32);
    }

    /// Returns the boolean pair of whether or not a track is continuously moving and discretely moving
    ///
    /// # Arguments
    /// None
    ///
    /// Note that tracks cannot be both continuously moving or discretely moving.
    /// enemies in the game should be continuously moving while menu cursors for example should be discretely moving
    ///
    pub fn moving_bool(&self) -> (bool, bool) {
        let continuous: bool;
        let discrete: bool;
        let move_count = (self.positions.len() - 1) as f32;
        let (cont_count, discrete_count) = self.moving_ratio();
        let (cont_freq, discrete_freq) = (cont_count / move_count, discrete_count / move_count);

        if cont_freq > 0.95 {
            continuous = true;
            discrete = false;
        } else if discrete_freq > 0.95 {
            discrete = true;
            continuous = false;
        } else {
            continuous = false;
            discrete = false;
        }
        return (continuous, discrete);
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
    pub fn blob_score_pair(t1: &SpriteTrack, t2: &SpriteTrack, lookback: usize, now: Time) -> f32 {
        // closeness score: 0 if touching over lookback and diff ID, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving
        let mut closeness = 0; // default not touching
        let mut same_spd = 0; // number of frames where they are moving at the same speed
        if t1.id != t2.id {
            // early return if now is younger than lookback
            // bad score if one of the tracks is younger than now.0 - lookback
            if now.0 < lookback
                || now.0 - lookback < t1.first_observation_time().0
                || now.0 - lookback < t2.first_observation_time().0
            {
                // TO DO fix
                return 100 as f32;
            }

            // consider some type of lower bound for the tracks 10-30 frames not enough to determine

            for n in (now.0 - lookback + 1)..now.0 {
                let mut vec1 = Vec::new();
                let mut vec2 = Vec::new();
                if let (Some((x1, y1)), Some((x1_p, y1_p))) =
                    (t1.position_at(Time(n)), t1.position_at(Time(n - 1)))
                {
                    let dispx = x1 - x1_p;
                    let dispy = y1 - y1_p;
                    vec1.push(dispx);
                    vec2.push(dispy);
                }
                if let (Some((x2, y2)), Some((x2_p, y2_p))) =
                    (t2.position_at(Time(n)), t2.position_at(Time(n - 1)))
                {
                    let dispx2 = x2 - x2_p;
                    let dispy2 = y2 - y2_p;
                    vec1.push(dispx2);
                    vec2.push(dispy2);
                }
                let rect1 = t1.getSpriteData().sprite_rect();
                let rect2 = t2.getSpriteData().sprite_rect();

                // closeness will be zero if they do overlap, if they don't overlap then closeness will be set to 100.
                if !rect1.expand(1).overlaps(&rect2.expand(1)) {
                    closeness = 100;
                }
                if vec1[0] == vec2[0] && vec1[1] == vec2[1] {
                    same_spd += 1;
                }
            }
        }
        let moving = 10.0 * (1.0 - same_spd as f32 / lookback as f32);
        return closeness as f32 + moving as f32;
    }

    pub fn blob_score(
        &self,
        t: &SpriteTrack,
        all_tracks: &[SpriteTrack],
        lookback: usize,
        time: Time,
    ) -> f32 {
        // closeness score: 0 if touching, 100 otherwise; use min among all self.live tracks with id != t.id
        // moving score: 10*proportion of frames over lookback moving by the same speed (assume no agreement for frames before t1 or t2 were alive)
        // closeness + moving
        // return min blob score of all of self.live_tracks with id != t.id

        if let Some(x) = self
            .live_tracks
            .iter()
            .map(|&tid| {
                let track = all_tracks.iter().find(|track| track.id == tid).unwrap();
                Self::blob_score_pair(track, t, lookback, time)
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap())
        {
            return x;
        } else {
            return 100 as f32; // not touching at all and not moving together at all
        }
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
                    // println!("Current position at ({},{})", bx, by);
                    (t, ax + bx / tl, ay + by / tl)
                }),
        );
        // let mut score = blob_score_pair(t1: &SpriteTrack, t2: &SpriteTrack, lookback: usize, now: Time)
    }
}
