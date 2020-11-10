use crate::screen::Screen;
use crate::tile::{TileChange, TileDB, TileGfxId};
use crate::Rect;

type RoomScreen = Screen<TileChange>;

pub struct Room {
    pub id: usize,
    pub screens: Vec<RoomScreen>,
}

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

    fn get_screen_for(&self, x: i32, y: i32) -> Option<usize> {
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
        // TODO any way to avoid bounds checking within these loops?
        // ul
        let ul = &mut self.screens[ul];
        for y in s.region.y..ud_split {
            for x in s.region.x..lr_split {
                extend_tile(ul, s, x, y, db);
            }
        }
        // ur
        let ur = &mut self.screens[ur];
        for y in s.region.y..ud_split {
            for x in lr_split..xmax {
                extend_tile(ur, s, x, y, db);
            }
        }
        // bl
        let bl = &mut self.screens[bl];
        for y in ud_split..ymax {
            for x in s.region.x..lr_split {
                extend_tile(bl, s, x, y, db);
            }
        }
        // br
        let br = &mut self.screens[br];
        for y in ud_split..ymax {
            for x in lr_split..xmax {
                extend_tile(br, s, x, y, db);
            }
        }
    }
}

#[inline(always)]
fn extend_tile(rs: &mut RoomScreen, s: &Screen<TileGfxId>, x: i32, y: i32, db: &mut TileDB) {
    assert!(s.region.contains(x, y), "{},{} : {:?}", x, y, s.region);
    assert!(rs.region.contains(x, y), "{},{} : {:?}", x, y, rs.region);
    rs.set(db.change_from_to(rs.get(x, y), s.get(x, y)), x, y);
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
