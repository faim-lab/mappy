use crate::framebuffer::Framebuffer;
use crate::pixels;
use std::hash::{Hash, Hasher};

pub trait Tile : PartialEq + Eq + Hash + Clone {
    fn empty() -> Self;
}


#[derive(Clone, Copy)]
pub struct TileGfx([u8; 8 * 8]);

impl TileGfx {
    pub fn read(fb: &Framebuffer, x: usize, y: usize) -> Self {
        let mut tile_data = [0_u8; 64];
        for yi in y..y + 8 {
            for xi in x..x + 8 {
                tile_data[(((yi - y) as u8) * 8 + ((xi - x) as u8)) as usize] =
                    fb.fb[fb.w * yi + xi];
            }
        }
        Self(tile_data)
    }
    pub fn write_rgb888(&self, buf: &mut [u8]) {
        assert!(buf.len() == self.0.len() * 3);
        for yi in 0..8 {
            for xi in 0..8 {
                let col = self.0[yi * 8 + xi];
                let (r, g, b) = pixels::rgb332_to_rgb888(col);
                buf[(yi * 8 + xi) * 3] = r;
                buf[(yi * 8 + xi) * 3 + 1] = g;
                buf[(yi * 8 + xi) * 3 + 2] = b;
            }
        }
    }
    pub fn perceptual_hash(&self) -> u128 {
        self.0.iter().fold(0_u128, |x,&y| x.wrapping_add(y as u128))
    }
}
impl PartialEq for TileGfx {
    fn eq(&self, other: &Self) -> bool {
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl Eq for TileGfx {}
impl Hash for TileGfx {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl Tile for TileGfx {
    fn empty() -> Self {
        TileGfx([0;8*8])
    }
}
