use super::{room::Room, Rect};

#[derive(Debug)]
struct RegisterRoom(usize, (i32, i32), f32);

pub struct Metaroom {
    id: usize,
    rooms: Vec<RegisterRoom>,
}

impl Metaroom {
    pub fn new(id: usize) -> Self {
        Self { id, rooms: vec![] }
    }
    pub fn merge_room(&mut self, id: usize, posn: (i32, i32), cost: f32) {
        match self.rooms.iter_mut().find(|RegisterRoom(i, _, _)| *i == id) {
            Some(RegisterRoom(_, p, c)) => {
                *p = posn;
                *c = cost;
            }
            None => self.rooms.push(RegisterRoom(id, posn, cost)),
        }
        println!("Merged {} into {} ({:?}@{})", id, self.id, posn, cost);
    }
    pub fn merge_metarooms(&self, metas:&[usize], all_metas:&[Self], rooms:&[Room]) -> Vec<(usize, (i32, i32), f32)> {
        // for each meta in metas
        //    for each room in this meta
        //       if it's cheap to merge it into self...
        //         then try it
        todo!()
    }
    pub fn merge_cost(
        &self,
        room: &Room,
        rooms: &[Room],
        mut threshold: f32,
    ) -> Option<((i32, i32), f32)> {

        // TODO a way to /unmerge/ rooms if need be?  If it starts out similar but seems different?

        // dbg!(room.id, rooms.len(), rooms.iter().map(|r| r.id).collect::<Vec<_>>());
        let rooms: Vec<_> = self
            .rooms
            .iter()
            .filter_map(|&RegisterRoom(rid, posn, _cost)| {
                if rid != room.id {
                    Some((posn, &rooms[rid]))
                } else {
                    None
                }
            })
            .collect();
        let rects: Vec<_> = rooms
            .iter()
            .map(|&((x, y), r)| Rect { x, y, ..r.region() })
            .collect();
        let full = if rooms.is_empty() {
            room.region()
        } else {
            rects.iter().skip(1).fold(rects[0], |r1, r2| r1.union(r2))
        };
        let room_r = room.region();
        let room_count = rooms.len();
        if room_count == 0 {
            return Some(((0, 0), 0.0));
        }
        let mut best = None;
        // dbg!(room_r, full, self.id);
        for y in (full.y - (room_r.h / 2) as i32)..(full.y + (full.h / 2) as i32) {
            for x in (full.x - (room_r.w / 2) as i32)..(full.x + (full.w / 2) as i32) {
                let mut cost = 0.0;
                for ((rx, ry), room_i) in rooms.iter() {
                    cost +=
                        room.merge_cost_at(x, y, *rx, *ry, room_i, threshold) / room_count as f32;
                    if cost > threshold {
                        break;
                    }
                }
                if cost < threshold {
                    threshold = cost;
                    best = Some(((x, y), cost));
                }
            }
        }
        // dbg!(self.id, best);
        best
        // for each registration of room.region() onto full, calculate difference across the rooms I have (going a row within each existing room at a time seems good, think about cache effects).  we want to take the best difference and throw away ones that get too bad.  One possibility is to go a row (or a room already in the metaroom, or a room/row combo) at a time and put that into a bnb kind of framework... since we want to find the best one.
        // min_by might work...? but it calculates everything.  I'd like to filter_map and then min_by maybe, or have the min_by sometimes choose to dump in the threshold value + 1.0

        // go through full, registering room at different offsets; bailing out each difference calculation once it gets too big (check every row or col or something).
        // see how room's set of seen changes intersects with each r in self.rooms's... if empty skip it
        // get cost of registering room onto it at best posn
        //   (this is the weighted average of cost of registering at posn wrt all other rooms in the metaroom)
        //      cost of registering ra in rb at posn is just existing room difference but with rects aligned appropriately and out of bounds spots ignored (also maybe taking change cycles into account)
        //   bail out if cost exceeds ROOM_MERGE_THRESHOLD
    }
}
