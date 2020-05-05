use retro_rs::Emulator;
use std::mem;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SpriteDataInternal {
    pub y: u8,
    pub pattern_id: u8,
    pub attrs: u8,
    pub x: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SpriteData {
    pub x: u8,
    pub y: u8,
    height: u8,
    pub pattern_id: u8,
    pub table: u8,
    pub attrs: u8,
}
#[allow(dead_code)]
impl SpriteData {
    pub fn width(self) -> u8 {
        8
    }
    pub fn height(self) -> u8 {
        self.height
    }
    pub fn vflip(self) -> bool {
        self.attrs & 0b1000_0000 != 0
    }
    pub fn hflip(self) -> bool {
        self.attrs & 0b0100_0000 != 0
    }
    pub fn bg(self) -> bool {
        self.attrs & 0b0010_0000 != 0
    }
    pub fn pal(self) -> u8 {
        4 + (self.attrs & 0b0000_0011)
    }
    pub fn is_valid(self) -> bool {
        self.y < 248
    }
    pub fn key(self) -> u32 {
        u32::from(self.pattern_id) | (u32::from(self.table) << 8)
    }
}
const SPRITE_SIZE: usize = 4;
pub const SPRITE_COUNT: usize = 0x100 / SPRITE_SIZE;
pub fn get_sprites(emu: &Emulator, sprites: &mut [SpriteData]) {
    assert_eq!(mem::size_of::<SpriteDataInternal>(), SPRITE_SIZE);
    let mut buf = [0; SPRITE_COUNT * SPRITE_SIZE];
    emu.get_system_ram(0x0200, SPRITE_COUNT * SPRITE_SIZE, &mut buf)
        .expect("Couldn't read RAM!");
    let ppuctrl = 0;
    // TODO put me back when the fceumm build goes up to buildbot
    // let ppuctrl = get_byte(emu, 0x2000);
    let sprite_height: u8 = if ((ppuctrl & 0b0010_0000) >> 5) == 1 {
        16
    } else {
        8
    };
    let table_bit = (ppuctrl & 0b0000_1000) >> 3;
    let buf: [SpriteDataInternal; SPRITE_COUNT] = unsafe { std::mem::transmute(buf) };
    assert_eq!(buf.len(), sprites.len());
    for (i, bs) in buf.iter().enumerate() {
        sprites[i] = SpriteData {
            x: bs.x,
            y: bs.y,
            height: sprite_height,
            pattern_id: bs.pattern_id,
            table: table_bit,
            attrs: bs.attrs,
        }
    }
}
