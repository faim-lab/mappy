use crate::screen::Screen;
use crate::tile::{TileChange, TileDB, TileGfxId};
use crate::Rect;
use std::collections::HashSet;

type RoomScreen = Screen<TileChange>;
#[derive(Clone)]
pub struct Room {
    pub id: usize,
    pub screens: Vec<RoomScreen>,
    pub seen_changes: HashSet<TileChange>,
    pub top_left: (i32, i32),
    pub bottom_right: (i32, i32),
}
// TODO consider dense grid of screens so that lookups are fast and predictable

impl Room {
    pub fn new(id: usize, screen: &Screen<TileGfxId>, db: &mut TileDB) -> Self {
        let mut ret = Self {
            id,
            screens: vec![Screen::new(
                Rect {
                    x: screen.region.x,
                    y: screen.region.y,
                    w: 32,
                    h: 32,
                },
                &db.get_initial_change(),
            )],
            seen_changes: HashSet::new(),
            top_left: (screen.region.x, screen.region.y),
            // TODO hacky, probably not right
            bottom_right: (screen.region.x + 1, screen.region.y + 1),
        };
        if screen.region.w != 0 && screen.region.h != 0 {
            ret.register_screen(&screen, db);
        }
        ret
    }
    pub fn width(&self) -> u32 {
        self.screens[0].region.w
    }
    pub fn height(&self) -> u32 {
        self.screens[0].region.h
    }
    pub fn region(&self) -> Rect {
        Rect {
            x: self.top_left.0,
            y: self.top_left.1,
            w: (self.bottom_right.0 - self.top_left.0) as u32,
            h: (self.bottom_right.1 - self.top_left.1) as u32,
        }
    }
    pub fn reregister_at(&mut self, x: i32, y: i32) {
        let Rect { x: ox, y: oy, w, h } = self.region();
        self.top_left = (x, y);
        self.bottom_right = (x + w as i32, y + h as i32);
        let xoff = x - ox;
        let yoff = y - oy;
        for s in self.screens.iter_mut() {
            s.reregister_at(s.region.x + xoff, s.region.y + yoff);
        }
    }
    pub fn get(&self, x: i32, y: i32) -> TileChange {
        self.screens[self
            .get_screen_for(x, y)
            .unwrap_or_else(|| panic!("bad {:?} {:?}", self.region(), (x, y)))]
        .get(x, y)
    }

    pub fn get_screen_for(&self, x: i32, y: i32) -> Option<usize> {
        self.screens.iter().position(|s| s.region.contains(x, y))
    }
    // x,y are in tile coordinates
    fn get_screen_for_or_add(&mut self, x: i32, y: i32, db: &TileDB) -> usize {
        if let Some(n) = self.get_screen_for(x, y) {
            return n;
        }
        let r0 = self.screens[0].region;
        let mut sx = r0.x;
        if x < r0.x {
            while x < sx {
                sx -= r0.w as i32;
            }
        } else {
            while sx + r0.w as i32 <= x {
                sx += r0.w as i32;
            }
        }
        let mut sy = r0.y;
        if y < r0.y {
            while y < sy {
                sy -= r0.h as i32;
            }
        } else {
            while sy + r0.h as i32 <= y {
                sy += r0.h as i32;
            }
        }
        if !Rect::new(sx, sy, r0.w, r0.h).contains(x, y) {
            println!(
                "Rect {},{} {},{} does not contain {},{}",
                sx, sy, r0.w, r0.h, x, y
            );
        }
        assert!(Rect::new(sx, sy, r0.w, r0.h).contains(x, y));
        assert!(self.get_screen_for(sx, sy).is_none());
        assert!(self.get_screen_for(sx + r0.w as i32 - 1, sy).is_none());
        assert!(self.get_screen_for(sx, sy + r0.h as i32 - 1).is_none());
        assert!(self
            .get_screen_for(sx + r0.w as i32 - 1, sy + r0.h as i32 - 1)
            .is_none());
        self.screens.push(Screen::new(
            Rect::new(sx, sy, r0.w, r0.h),
            &db.get_initial_change(),
        ));

        //println!("Added region {:?}", self.screens.last().unwrap().region);
        assert_eq!(self.get_screen_for(x, y).unwrap(), self.screens.len() - 1);
        self.screens.len() - 1
    }
    // r is presumed to be in tile coordinates
    fn gather_screens(&mut self, r: Rect, db: &TileDB) -> (usize, usize, usize, usize) {
        (
            self.get_screen_for_or_add(r.x, r.y, db),
            self.get_screen_for_or_add(r.x + r.w as i32 - 1, r.y, db),
            self.get_screen_for_or_add(r.x, r.y + (r.h as i32) - 1, db),
            self.get_screen_for_or_add(r.x + (r.w as i32) - 1, r.y + (r.h as i32) - 1, db),
        )
    }
    pub fn register_screen(&mut self, s: &Screen<TileGfxId>, db: &mut TileDB) {
        let (ul, ur, bl, br) = self.gather_screens(s.region, db);
        // Four loops: the ul part, the ur part, the bl part, the br part.
        // ul is s.y..(s.y+s.h).min(ul.y+ul.h)
        let xmax = s.region.x + s.region.w as i32;
        let ymax = s.region.y + s.region.h as i32;
        let ulr = self.screens[ul].region;
        let lr_split = xmax.min(ulr.x + ulr.w as i32);
        let ud_split = ymax.min(ulr.y + ulr.h as i32);
        let mut seen = Vec::with_capacity(s.region.w as usize * s.region.h as usize);
        // TODO any way to avoid bounds checking within these loops?
        // ul
        let ul = &mut self.screens[ul];
        for y in s.region.y..ud_split {
            for x in s.region.x..lr_split {
                extend_tile(ul, s, &mut seen, x, y, db);
            }
        }
        // ur
        let ur = &mut self.screens[ur];
        for y in s.region.y..ud_split {
            for x in lr_split..xmax {
                extend_tile(ur, s, &mut seen, x, y, db);
            }
        }
        // bl
        let bl = &mut self.screens[bl];
        for y in ud_split..ymax {
            for x in s.region.x..lr_split {
                extend_tile(bl, s, &mut seen, x, y, db);
            }
        }
        // br
        let br = &mut self.screens[br];
        for y in ud_split..ymax {
            for x in lr_split..xmax {
                extend_tile(br, s, &mut seen, x, y, db);
            }
        }
        self.seen_changes.extend(seen.into_iter());
        self.top_left.0 = self.top_left.0.min(s.region.x);
        self.top_left.1 = self.top_left.1.min(s.region.y);
        self.bottom_right.0 = self.bottom_right.0.max(xmax);
        self.bottom_right.1 = self.bottom_right.1.max(ymax);
    }
    pub fn merge_cost_at(
        &self,
        x: i32,
        y: i32,
        r2xo: i32,
        r2yo: i32,
        room: &Room,
        tiles: &TileDB,
        threshold: f32,
    ) -> f32 {
        let mut any1 = 0;
        let mut any2 = 0;
        let r = self.region();
        let r2x = r2xo + x;
        let r2y = r2yo + y;
        let mut cost = 0.0;
        //println!("{:?}-{:?}\n{:?}-{:?}",r, (x, y), room.region(), (rxo, ryo));
        for yo in 0..(r.h as i32) {
            for xo in 0..(r.w as i32) {
                // TODO make this more cache friendly, should be able to read a row at a time; room could be a different data structure?
                let s1x = r.x + xo;
                let s1y = r.y + yo;
                let screen = self.get_screen_for(s1x, s1y);
                let s2x = r2x + xo;
                let s2y = r2y + yo;
                let screen2 = room.get_screen_for(s2x, s2y);
                any1 += if screen.is_some() { 1 } else { 0 };
                any2 += if screen2.is_some() { 1 } else { 0 };
                assert!(
                    screen.is_some(),
                    "r1 {:?}\noff {},{}\nr2 {:?}\noff {},{}\nat {},{}\nposns {:?} -vs- {:?}",
                    self.region(),
                    x,
                    y,
                    room.region(),
                    r2x,
                    r2y,
                    xo,
                    yo,
                    (s1x, s1y),
                    (s2x, s2y)
                );
                cost += match (screen, screen2) {
                    (Some(screen), Some(screen2)) => {
                        // println!("compare");
                        // TODO if tiles.compatible(..., ...)
                        tiles.change_cost(
                            self.screens[screen].get(s1x, s1y),
                            room.screens[screen2].get(s2x, s2y),
                        )
                    }
                    _ => 0.0,
                }
            }
            if cost > threshold {
                break;
            }
        }
        assert!(
            any1 > 0,
            "a1 {:?}-{:?} {:?} {:?}",
            r,
            (x, y),
            room.region(),
            cost
        );
        assert!(
            any2 > 0,
            "a2 {:?}-{:?} {:?} {:?} {:?}",
            r,
            (x, y),
            (r2x, r2y),
            room.region(),
            cost
        );
        cost
    }
}

#[inline(always)]
fn extend_tile(
    rs: &mut RoomScreen,
    s: &Screen<TileGfxId>,
    seen: &mut Vec<TileChange>,
    x: i32,
    y: i32,
    db: &mut TileDB,
) {
    assert!(s.region.contains(x, y), "{},{} : {:?}", x, y, s.region);
    assert!(rs.region.contains(x, y), "{},{} : {:?}", x, y, rs.region);
    if s.get(x, y) != db.get_initial_tile() {
        let change = db.change_from_to(rs.get(x, y), s.get(x, y));
        seen.push(change);
        rs.set(change, x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_screen_for() {
        let mut db = TileDB::new();
        let r0 = Rect::new(5, 5, 20, 20);
        let mut r = Room::new(0, &Screen::new(r0, &db.get_initial_tile()), &mut db);
        r.get_screen_for_or_add(5, 5, &db);
        assert_eq!(r.screens[r.get_screen_for(5, 5).unwrap()].region, r0);
        assert_eq!(r.screens.len(), 1);
        r.get_screen_for_or_add(24, 24, &db);
        assert_eq!(r.screens[r.get_screen_for(24, 24).unwrap()].region, r0);
        assert_eq!(r.screens.len(), 1);
        r.get_screen_for_or_add(25, 25, &db);
        assert_eq!(
            r.screens[r.get_screen_for(25, 25).unwrap()].region,
            Rect::new(r0.x + r0.w as i32, r0.y + r0.h as i32, r0.w, r0.h)
        );
        assert_eq!(r.screens.len(), 2);
        dbg!(r.screens.iter().map(|s| s.region).collect::<Vec<_>>());
        r.get_screen_for_or_add(0, 0, &db);
        assert_eq!(
            r.screens[r.get_screen_for(0, 0).unwrap()].region,
            Rect::new(r0.x - r0.w as i32, r0.y - r0.h as i32, r0.w, r0.h)
        );
        assert_eq!(r.screens.len(), 3);
        r.get_screen_for_or_add(-10, -10, &db);
        assert_eq!(
            r.screens[r.get_screen_for(-10, -10).unwrap()].region,
            Rect::new(r0.x - r0.w as i32, r0.y - r0.h as i32, r0.w, r0.h)
        );
        assert_eq!(r.screens.len(), 3);
        r.get_screen_for_or_add(-30, -30, &db);
        assert_eq!(
            r.screens[r.get_screen_for(-30, -30).unwrap()].region,
            Rect::new(r0.x - r0.w as i32 * 2, r0.y - r0.h as i32 * 2, r0.w, r0.h)
        );
        assert_eq!(r.screens.len(), 4);
    }

    #[test]
    fn test_get_screen_for_2() {
        let mut db = TileDB::new();
        let r0 = Rect::new(2, 29, 29, 27);
        let mut r = Room::new(0, &Screen::new(r0, &db.get_initial_tile()), &mut db);
        r.get_screen_for_or_add(-56, 29, &db);
        r.get_screen_for_or_add(-27, 29, &db);
        assert_eq!(r.screens.len(), 3);
    }

    #[test]
    fn test_register() {
        use crate::tile::{TileGfx, TILE_NUM_PX};
        let mut db = TileDB::new();
        let r0 = Rect::new(5, 5, 20, 20);
        let t0 = db.get_initial_tile();
        let t1 = db.get_tile(TileGfx([1; TILE_NUM_PX]));
        let s = Screen::new(r0, &t1);
        let mut r = Room::new(0, &s, &mut db);

        assert_eq!(r.screens.len(), 1);
        for y in s.region.y..(s.region.y + s.region.h as i32) {
            for x in s.region.x..(s.region.x + s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x, y).unwrap()].get(x, y);
                assert_eq!(atile.from, t0);
                assert_eq!(atile.to, t1);
            }
        }
        r.register_screen(&s, &mut db);
        assert_eq!(r.screens.len(), 1);
        for y in s.region.y..(s.region.y + s.region.h as i32) {
            for x in s.region.x..(s.region.x + s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x, y).unwrap()].get(x, y);
                assert_eq!(atile.from, t0);
                assert_eq!(atile.to, t1);
            }
        }

        let t2 = db.get_tile(TileGfx([2; TILE_NUM_PX]));
        let s = Screen::new(
            Rect::new(r0.x - r0.w as i32 / 2, r0.y + r0.h as i32 / 2, r0.w, r0.h),
            &t2,
        );
        r.register_screen(&s, &mut db);

        assert_eq!(r.screens.len(), 4);
        for y in s.region.y..(s.region.y + s.region.h as i32) {
            for x in s.region.x..(s.region.x + s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x, y).unwrap()].get(x, y);
                if x < r0.x || y >= r0.y + (r0.h as i32) {
                    assert_eq!(atile.from, t0);
                    assert_eq!(atile.to, t2);
                } else {
                    assert_eq!(atile.from, t1);
                    assert_eq!(atile.to, t2);
                }
            }
        }
    }
}
