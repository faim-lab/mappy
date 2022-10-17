use crate::tile::Tile;
use crate::Rect;

#[derive(Clone)]
pub struct Screen<T: Tile> {
    pub region: Rect,
    tiles: Box<[T]>,
}

impl<T: Tile> std::ops::Index<(i32, i32)> for Screen<T> {
    type Output = T;
    #[inline(always)]
    fn index(&self, (x, y): (i32, i32)) -> &Self::Output {
        &self.tiles[((y - self.region.y) * self.region.w as i32 + x - self.region.x) as usize]
    }
}

impl<T: Tile> Screen<T> {
    pub fn new(region: Rect, tile: T) -> Self {
        Self {
            region,
            tiles: vec![tile; region.w as usize * region.h as usize].into_boxed_slice(),
        }
    }
    #[inline(always)]
    pub fn get(&self, x: i32, y: i32) -> Option<T> {
        self.tiles
            .get(((y - self.region.y) * self.region.w as i32 + x - self.region.x) as usize)
            .copied()
    }
    #[inline(always)]
    pub fn set(&mut self, t: T, x: i32, y: i32) {
        self.tiles[((y - self.region.y) * self.region.w as i32 + x - self.region.x) as usize] = t;
    }
    pub fn combine(screens: Vec<Screen<T>>, init: T) -> Screen<T> {
        let mut r = screens[0].region;
        for Screen { region, .. } in screens.iter().skip(1) {
            r = r.union(region);
        }
        let mut s = Screen::new(r, init);
        for s2 in screens {
            for y in s2.region.y..(s2.region.y + s2.region.h as i32) {
                for x in s2.region.x..(s2.region.x + s2.region.w as i32) {
                    s.set(s2[(x, y)], x, y);
                }
            }
        }
        s
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
        let shared = self.region.intersection(&s.region).unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
        let mut diff = (everything.w * everything.h - shared.w * shared.h) as f32;
        for y in shared.y..(shared.y + shared.h as i32) {
            for x in shared.x..(shared.x + shared.w as i32) {
                diff += if self.get(x, y) == s.get(x, y) {
                    0.0
                } else {
                    1.0
                };
            }
        }
        // dbg!(diff, self.region, s.region, everything);
        diff
    }
}
