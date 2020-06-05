use retro_rs::Emulator;
use std::mem;
use crate::Time;
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
    pub fn distance(&self, other:&Self) -> f32 {
        let dx = other.x as f32-self.x as f32;
        let dy = other.y as f32-self.y as f32;
        (dx*dx+dy*dy).sqrt()
    }
}
const SPRITE_SIZE: usize = 4;
pub const SPRITE_COUNT: usize = 0x100 / SPRITE_SIZE;
pub fn get_sprites(emu: &Emulator, sprites: &mut [SpriteData]) {
    let buf = &emu.system_ram_ref()[0x0200..0x0200+SPRITE_COUNT*SPRITE_SIZE];
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
        let [y,pattern_id,attrs,x] = match *bs {
            [y,pattern_id,attrs,x] => [y,pattern_id,attrs,x],
            _ => unreachable!()
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
pub fn overlapping_sprite(x: usize, y: usize, w:usize, h:usize, sprites: &[SpriteData]) -> bool {
    for s in sprites.iter().filter(|s| s.is_valid()) {
        if x <= s.x as usize + s.width() as usize
            && s.x as usize <= x + w
            // this is because a sprite is drawn on the scanline -after- its y value? I think?
            && y <= (s.y+1) as usize + s.height() as usize
            // could be s.y+1 but we'll keep it more generous just to be safe
            && s.y as usize <= y + h
        {
            return true;
        }
    }
    false
}

#[derive(Clone)]
pub struct SpriteTrack {
    pub positions:Vec<(Time,(i32,i32),SpriteData)>,
    // TODO measure against vecs or even arrays?
    pub patterns:HashSet<u8>,
    pub tables:HashSet<u8>,
    pub attrs:HashSet<u8>
}

impl SpriteTrack {
    pub fn new(t:Time, scroll:(i32,i32), sd:SpriteData) -> Self {
        let mut ret = Self {
            positions:vec![],
            patterns:HashSet::new(),
            tables:HashSet::new(),
            attrs:HashSet::new()
        };
        ret.update(t, scroll, sd);
        ret
    }
    pub fn current_data(&self) -> &SpriteData {
        &self.positions[self.positions.len()-1].2
    }
    pub fn last_observation_time(&self) -> Time {
        self.positions[self.positions.len()-1].0
    }
    pub fn update(&mut self, t:Time, scroll:(i32,i32), sd:SpriteData) {
        // TODO handle time properly, dedup if no change
        self.positions.push((t,scroll,sd));
        self.patterns.insert(sd.pattern_id);
        self.tables.insert(sd.table);
        self.attrs.insert(sd.attrs);
    }
    pub fn starting_point(&self) -> (i32,i32) {
        let (_, (sx,sy), sd) = &self.positions[0];
        (sx+sd.x as i32, sy+sd.y as i32)
    }
    pub fn seen_pattern(&self, pat:u8) -> bool {
        self.patterns.contains(&pat)
    }
    pub fn seen_table(&self, tab:u8) -> bool {
        self.tables.contains(&tab)
    }
    pub fn seen_attrs(&self, attrs:u8) -> bool {
        self.attrs.contains(&attrs)
    }
}
