use crate::tile::Tile;
use crate::Rect;

#[derive(Clone)]
pub struct Screen<T: Tile> {
    pub region: Rect,
    tiles: Box<[T]>,
}

impl<T: Tile> Screen<T> {
    pub fn new(region: Rect, tile: &T) -> Self {
        Self {
            region,
            tiles: vec![*tile; region.w as usize * region.h as usize].into_boxed_slice(),
        }
    }
    #[inline(always)]
    pub fn get(&self, x: i32, y: i32) -> T {
        self.tiles[((y - self.region.y) * self.region.w as i32 + x - self.region.x) as usize]
    }
    #[inline(always)]
    pub fn set(&mut self, t: T, x: i32, y: i32) {
        self.tiles[((y - self.region.y) * self.region.w as i32 + x - self.region.x) as usize] = t;
    }
    pub fn reregister_at(&mut self, x: i32, y: i32) {
        self.region.x = x;
        self.region.y = y;
    }
    pub fn copy_from(&mut self, s: &Self) {
        if self.region.w != s.region.w || self.region.h != s.region.h {
            self.tiles = s.tiles.clone();
        } else {
            self.tiles.copy_from_slice(&s.tiles);
        }
        self.region = s.region;
    }
    pub fn difference(&self, s: &Self) -> f32 {
        // take union of my rect and s's rect
        // add up differences for each tile in that union
        let everything = self.region.union(&s.region);
        let mut diff = 0.0;
        for y in everything.y..(everything.y + everything.h as i32) {
            for x in everything.x..(everything.x + everything.w as i32) {
                let in_self = self.region.contains(x, y);
                let in_s = s.region.contains(x, y);
                diff += if in_self && in_s {
                    if self.get(x, y) == s.get(x, y) {
                        0.0
                    } else {
                        1.0
                    }
                } else if in_s || in_self {
                    1.0
                } else {
                    0.0
                };
            }
        }
        // dbg!(diff, self.region, s.region, everything);
        diff
    }
}
