use crate::tile::Tile;
use crate::Rect;

pub struct Screen<T:Tile> {
    region: Rect,
    tiles: Vec<T>
}

impl<T : Tile> Screen<T> {
    pub fn new(region:Rect) -> Self {
        Self { region, tiles:vec![T::empty(); region.w as usize*region.h as usize] }
    }
    pub fn get(&self, x:u32, y:u32) -> &T {
        &self.tiles[(y*self.region.w+x) as usize]
    }
    pub fn set(&mut self, t:T, x:u32, y:u32) {
        self.tiles[(y*self.region.w+x) as usize] = t;
    }
}
