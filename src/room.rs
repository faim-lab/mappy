use crate::tile::{TileChange, TileGfxId, TileDB};
use crate::screen::Screen;
use crate::Rect;

type RoomScreen = Screen<TileChange>;

pub struct Room {
    pub id : usize,
    screens : Vec<RoomScreen>
}

impl Room {
    pub fn new(id:usize, screen:&Screen<TileGfxId>, db:&mut TileDB) -> Self {
        let mut ret = Self {
            id,
            screens:vec![Screen::new(screen.region, &db.get_initial_change())]
        };
        if screen.region.w != 0 && screen.region.h != 0 {
            ret.register_screen(&screen, db);
        }
        ret
    }
        fn get_screen_for(&self, x:i32, y:i32) -> Option<usize> {
            self.screens.iter().position(|s| s.region.contains(x,y))
    }
    // x,y are in tile coordinates
    fn get_screen_for_or_add(&mut self, x:i32, y:i32, db:&TileDB) -> usize {
        if let Some(n) = self.get_screen_for(x,y) {
            return n
        }
        let r0 = self.screens[0].region;
        let x_off = x-r0.x;
        // find smallest multiple of r0.w, offset r.x back
        let mut x_place = r0.x + (x_off/r0.w as i32)*r0.w as i32;
        // subtract another r0.w if x is negative
        if x < r0.x { x_place -= r0.w as i32; }
        let y_off = y-r0.y;
        // find smallest multiple of r0.h, offset r.y back
        let mut y_place = r0.y + (y_off/r0.h as i32)*r0.h as i32;
        // subtract another r0.h if y is negative
        if y < r0.y { y_place -= r0.h as i32; }
        self.screens.push(Screen::new(Rect::new(x_place, y_place, r0.w, r0.h), &db.get_initial_change()));
        self.screens.len()-1
    }
    // r is presumed to be in tile coordinates
    fn gather_screens(&mut self, r:Rect, db:&TileDB) -> (usize, usize, usize, usize) {
        (
            self.get_screen_for_or_add(r.x,r.y,db),
            self.get_screen_for_or_add(r.x+r.w as i32-1, r.y, db),
            self.get_screen_for_or_add(r.x, r.y+(r.h as i32)-1, db),
            self.get_screen_for_or_add(r.x+(r.w as i32)-1, r.y+(r.h as i32)-1, db)
        )
    }
    pub fn register_screen(&mut self, s:&Screen<TileGfxId>, db:&mut TileDB) {
        let (ul,ur,bl,br) = self.gather_screens(s.region, db);
        // Four loops: the ul part, the ur part, the bl part, the br part.
        // ul is s.y..(s.y+s.h).min(ul.y+ul.h)
        let xmax = s.region.x+s.region.w as i32;
        let ymax = s.region.y+s.region.h as i32;
        let ul = &mut self.screens[ul];
        let lr_split = xmax.min(ul.region.x+ul.region.w as i32);
        let ud_split = ymax.min(ul.region.y+ul.region.h as i32);
        // ul
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
fn extend_tile(rs:&mut RoomScreen, s:&Screen<TileGfxId>,
               x:i32, y:i32,
               db:&mut TileDB) {
    assert!(s.region.contains(x,y));
    assert!(rs.region.contains(x,y));
    rs.set(db.change_from_to(rs.get(x,y), s.get(x,y)), x, y);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_screen_for() {
        let mut db = TileDB::new();
        let r0 = Rect::new(5, 5, 20, 20);
        let mut r = Room::new(0, &Screen::new(r0, &db.get_initial_tile()), &mut db);
        r.get_screen_for_or_add(5,5,&db);
        assert_eq!(r.screens[r.get_screen_for(5,5).unwrap()].region, r0);
        assert_eq!(r.screens.len(), 1);
        r.get_screen_for_or_add(24,24,&db);
        assert_eq!(r.screens[r.get_screen_for(24,24).unwrap()].region, r0);
        assert_eq!(r.screens.len(), 1);
        r.get_screen_for_or_add(25,25,&db);
        assert_eq!(r.screens[r.get_screen_for(25,25).unwrap()].region, Rect::new(r0.x+r0.w as i32, r0.y+r0.h as i32, r0.w, r0.h));
        assert_eq!(r.screens.len(), 2);
        dbg!(r.screens.iter().map(|s|s.region).collect::<Vec<_>>());
        r.get_screen_for_or_add(0,0,&db);
        assert_eq!(r.screens[r.get_screen_for(0,0).unwrap()].region, Rect::new(r0.x-r0.w as i32, r0.y-r0.h as i32, r0.w, r0.h));
        assert_eq!(r.screens.len(), 3);
        r.get_screen_for_or_add(-10,-10,&db);
        assert_eq!(r.screens[r.get_screen_for(-10,-10).unwrap()].region, Rect::new(r0.x-r0.w as i32, r0.y-r0.h as i32, r0.w, r0.h));
        assert_eq!(r.screens.len(), 3);
        r.get_screen_for_or_add(-30,-30,&db);
        assert_eq!(r.screens[r.get_screen_for(-30,-30).unwrap()].region, Rect::new(r0.x-r0.w as i32*2, r0.y-r0.h as i32*2, r0.w, r0.h));
        assert_eq!(r.screens.len(), 4);
    }

    #[test]
    fn test_register() {
        use crate::tile::TileGfx;
        let mut db = TileDB::new();
        let r0 = Rect::new(5, 5, 20, 20);
        let t0 = db.get_initial_tile();
        let t1 = db.get_tile(TileGfx([1;8*8]));
        let s = Screen::new(r0, &t1);
        let mut r = Room::new(0, &s, &mut db);

        assert_eq!(r.screens.len(), 1);
        for y in s.region.y..(s.region.y+s.region.h as i32) {
            for x in s.region.x..(s.region.x+s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x,y).unwrap()].get(x,y);
                assert_eq!(atile.from, t0);
                assert_eq!(atile.to, t1);
            }
        }
        r.register_screen(&s, &mut db);
        assert_eq!(r.screens.len(), 1);
        for y in s.region.y..(s.region.y+s.region.h as i32) {
            for x in s.region.x..(s.region.x+s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x,y).unwrap()].get(x,y);
                assert_eq!(atile.from, t0);
                assert_eq!(atile.to, t1);
            }
        }

        let t2 = db.get_tile(TileGfx([2;8*8]));
        let s = Screen::new(Rect::new(r0.x-r0.w as i32/2, r0.y+r0.h as i32/2, r0.w, r0.h), &t2);
        r.register_screen(&s, &mut db);

        assert_eq!(r.screens.len(), 4);
        for y in s.region.y..(s.region.y+s.region.h as i32) {
            for x in s.region.x..(s.region.x+s.region.w as i32) {
                let atile = r.screens[r.get_screen_for(x,y).unwrap()].get(x,y);
                if x < r0.x || y >= r0.y+(r0.h as i32) {
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
