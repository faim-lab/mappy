use crate::tile::Tile;
use crate::Rect;

pub struct Screen<T:Tile> {
    pub region: Rect,
    tiles: Box<[T]>
}

impl<T : Tile> Screen<T> {
    pub fn new(region:Rect, tile:&T) -> Self {
        Self {
            region,
            tiles:
            vec![tile.clone();region.w as usize*region.h as usize].into_boxed_slice()
        }
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
