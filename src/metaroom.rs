use super::{room::Room, Rect};

// // the usize here is another metaroom
// #[derive(Debug)]
// pub struct RegisterRoom(usize, (i32, i32), f32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaroomID(pub usize);

#[derive(Debug, Clone)]
pub struct Metaroom {
    pub id: MetaroomID,
    pub registrations: Vec<(usize, (i32, i32))>,
    pub merged_into: Vec<MetaroomID>,
}

impl Metaroom {
    fn new_single(id: MetaroomID, rid: usize) -> Self {
        Self {
            id,
            registrations: vec![(rid, (0, 0))],
            merged_into: vec![],
        }
    }
    fn new_merge(id: MetaroomID, registrations: Vec<(usize, (i32, i32))>) -> Self {
        Self {
            id,
            registrations,
            merged_into: vec![],
        }
    }
    pub fn region(&self, rooms: &[Room]) -> Rect {
        let (r0, p0) = self.registrations[0];
        let r0 = &rooms[r0];
        let mut r = Rect {
            x: p0.0,
            y: p0.1,
            ..r0.region()
        };
        for &(room, (x, y)) in self.registrations.iter() {
            let r2 = Rect {
                x,
                y,
                ..rooms[room].region()
            };
            r = r.union(&r2);
        }
        r
    }
}

#[derive(Clone)]
pub struct Merges {
    // nodes
    metarooms: Vec<Metaroom>,
}

impl Merges {
    pub fn new() -> Self {
        Self { metarooms: vec![] }
    }
    pub fn len(&self) -> usize {
        self.metarooms.len()
    }
    pub fn metaroom(&self, id: usize) -> &Metaroom {
        self.metarooms.iter().find(|&mr| mr.id.0 == id).unwrap()
    }
    pub fn metaroom_mut(&mut self, id: MetaroomID) -> &mut Metaroom {
        self.metarooms.iter_mut().find(|mr| mr.id == id).unwrap()
    }
    pub fn metarooms(&self) -> impl Iterator<Item = &Metaroom> {
        self.all_metarooms()
            .take_while(|mr| mr.merged_into.is_empty())
    }
    pub fn all_metarooms(&self) -> impl Iterator<Item = &Metaroom> {
        self.metarooms.iter()
    }
    pub fn merge_new_room(
        &mut self,
        room: usize,
        merges: &[(MetaroomID, (i32, i32), f32)],
    ) -> MetaroomID {
        println!("Final merge {}->{:?}", room, merges);
        if merges.is_empty() {
            let mid = MetaroomID(self.metarooms.len());
            let meta = Metaroom::new_single(mid, room);
            // definitely still sorted!
            println!("pushed meta a {:?}", mid);
            self.metarooms.insert(0, meta);
            return mid;
        }
        // add an arrow from every metaroom in merges and from room up to a new metaroom node
        //add room to merged_into of everything in merges
        let mid = MetaroomID(self.metarooms.len() + 1);

        {
            let room_mid = MetaroomID(self.metarooms.len());
            let mut meta = Metaroom::new_single(room_mid, room);
            meta.merged_into.push(mid);
            println!("pushed meta b {:?}", mid);
            self.metarooms.insert(0, meta);
        }

        let mut regs = Vec::with_capacity(merges.len() + 1);
        regs.push((room, (0, 0)));
        // is this right?
        for (mri, (rx, ry), _) in merges.iter() {
            let meta = self.metaroom_mut(*mri);
            for (rid, (rrx, rry)) in meta.registrations.iter() {
                regs.push((*rid, (rrx - rx, rry - ry)));
            }
            meta.merged_into.push(mid);
        }
        println!("pushed meta c {:?} {:?}", mid, regs);

        self.metarooms.push(Metaroom::new_merge(mid, regs));
        //resort everything
        self.metarooms.sort_unstable_by_key(|m| m.merged_into.len());
        mid
    }
}
