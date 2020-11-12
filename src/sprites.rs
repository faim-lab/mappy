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
    pub fn get_sprite_data(&self) -> &SpriteData {
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
    pub fn move_count(&self) -> i32 {
        // this function calculates the number of moves of a track.
        // iterate through the positions property
        // calculate the distance between a position and the position one index after it
        // if the distance is greater than 0 then we consider the track position to have moved.
        // increment counter
        let mut count = 0;

        for i in 0..(self.positions.len() - 1) {
            let dist = Self::distance(self, i, i + 1);
            // when do we increment count??
            if dist > 0.0 {
                count = count + 1;
            }
        }

        return count as i32;
    }
    pub fn continuous_move_ratio(&self, threshold: usize) -> f32 {
        // this function computes the ratio that looks at the changes in the tracks' position over time.

        // 0 - 10 stationary track: the positions rarely move
        // ex: a green pipe
        // we get this score if the track has a low move count

        // 11 - 69 discretely warping: the positions jump in big jumps
        // ex: a scoreboard jumping from position to position
        // we get this score if the track has a relatively high move count but we deduct points in these scenarios:
        // .. the distance between the two positions surpass a given threshold
        // .. the distances between two positions consistantly jump in 8+ pixels.
        //

        // 70 - 100 if continuously moving: goomba, fireballs,
        let total_positions = self.positions.len() - 1; // total position changes
        let mut move_count = self.move_count(); // total moves or actual changes in distance

        for i in 0..total_positions {
            let At(_, (sx, sy), sd) = &self.positions[i];
            let At(_, (sx2, sy2), sd2) = &self.positions[i + 1];
            let x1 = sx + sd.x as i32;
            let x2 = sx2 + sd2.x as i32;
            let y1 = sy + sd.y as i32;
            let y2 = sy2 + sd2.y as i32;
            let dispx = x1 - x2;
            let dispy = y1 - y2;
            let distance = ((dispx * dispx + dispy * dispy) as f32).sqrt();

            // if the position move in 8+ pixel jumps than decrease the move count
            if distance > threshold as f32 {
                move_count = move_count - 1;
            }
        }

        return move_count as f32 / total_positions as f32;
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
                let rect1 = t1.get_sprite_data().sprite_rect();
                let rect2 = t2.get_sprite_data().sprite_rect();

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
