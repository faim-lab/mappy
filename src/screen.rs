use crate::tile::Tile;
use crate::Rect;

pub struct Screen<T:Tile> {
    pub region: Rect,
    tiles: Vec<T>
}

impl<T : Tile> Screen<T> {
    pub fn new(region:Rect) -> Self {
        Self { region, tiles:vec![T::empty(); region.w as usize*region.h as usize] }
    }
    #[inline(always)]
    pub fn get(&self, x:i32, y:i32) -> &T {
        &self.tiles[((y-self.region.y)*self.region.w as i32+x-self.region.x) as usize]
    }
    #[inline(always)]
    pub fn set(&mut self, t:T, x:i32, y:i32) {
        self.tiles[((y-self.region.y)*self.region.w as i32+x-self.region.x) as usize] = t;
    }
}
