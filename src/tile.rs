use crate::framebuffer::Framebuffer;
use id_arena::{Arena, Id};
use retro_rs::pixels;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};

pub trait Tile: PartialEq + Eq + Hash + Clone + Copy {}

pub const TILE_SIZE: usize = 8;
pub const TILE_NUM_PX: usize = TILE_SIZE * TILE_SIZE;
// TODO consider 16x16 or parameterizable

#[derive(Clone, Copy)]
pub struct TileGfx(pub [u8; TILE_NUM_PX]);

impl TileGfx {
    // TODO if profiling shows tile creation is hot, replace with a cache friendlier api with read_row(&mut self, x, y, row) so we can read a whole framebuffer row off at a time
    pub fn read(fb: &Framebuffer, x: usize, y: usize) -> Self {
        let mut tile_data = [0_u8; TILE_NUM_PX];
        assert!(fb.w * (y + TILE_SIZE) <= fb.fb.len());
        let rows = &fb.fb[fb.w * y..fb.w * (y + TILE_SIZE)];
        for (yi, row) in rows.chunks_exact(fb.w).enumerate() {
            let cols = &row[x..x + TILE_SIZE];
            tile_data[(yi * TILE_SIZE)..((yi + 1) * TILE_SIZE)].copy_from_slice(cols);
        }
        Self(tile_data)
    }
    pub fn write_rgb888(&self, buf: &mut [u8]) {
        assert!(buf.len() == self.0.len() * 3);
        for (col, dst) in self.0.iter().zip(buf.chunks_exact_mut(3)) {
            let (r, g, b) = pixels::rgb332_to_rgb888(*col);
            dst[0] = r;
            dst[1] = g;
            dst[2] = b;
        }
    }
    pub fn write_rgb888_at(&self, x: usize, y: usize, buf: &mut [u8], buf_w: usize) {
        assert!((x + TILE_SIZE) <= buf_w);
        for (row_t, row_b) in self
            .0
            .chunks_exact(TILE_SIZE)
            .zip(buf[(y * 3 * buf_w)..((y + TILE_SIZE) * buf_w * 3)].chunks_mut(buf_w * 3))
        {
            for (col, dst) in row_t
                .iter()
                .zip(row_b[(x * 3)..(x * 3 + TILE_SIZE * 3)].chunks_mut(3))
            {
                let (r, g, b) = pixels::rgb332_to_rgb888(*col);
                dst[0] = r;
                dst[1] = g;
                dst[2] = b;
            }
        }
    }

    pub fn perceptual_hash(&self) -> u128 {
        self.0
            .iter()
            .fold(0_u128, |x, &y| x.wrapping_add(y as u128))
    }
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self([0; TILE_NUM_PX])
    }
}
impl PartialEq for TileGfx {
    fn eq(&self, other: &Self) -> bool {
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }
}
impl Eq for TileGfx {}
impl Hash for TileGfx {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl fmt::Debug for TileGfx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileGfx")
            .field("phash", &self.perceptual_hash())
            .finish()
    }
}

pub type TileGfxId = Id<TileGfx>;
impl Tile for TileGfxId {}

use std::collections::HashSet;
struct Chain<T: Ord + Eq + Copy> {
    elts: Vec<T>,
    // TODO this is actually bad, there must be a bug since metroid is showing many tiles with >1200 predecessors in the chain even though there are only 624 tiles and somehow 2453 changes (seems like too many changes)
    fwd: Vec<HashSet<usize>>,
    back: Vec<HashSet<usize>>,
}
impl<T> Chain<Id<T>> {
    fn new() -> Self {
        Self {
            elts: vec![],
            fwd: vec![],
            back: vec![],
        }
    }
    fn insert(&mut self, t: Id<T>) {
        if t.index() == self.elts.len() {
            self.elts.push(t);
            self.fwd.push(HashSet::new());
            self.back.push(HashSet::new());
        } else {
            assert!(t.index() < self.elts.len());
        }
    }
    fn chain(&mut self, tpre: Id<T>, tpost: Id<T>) {
        // TODO, if tpre is initial, should this just be ignored?
        let ipre = tpre.index();
        if ipre == 0 { return; }
        let ipost = tpost.index();
        self.fwd[ipre].insert(ipost);
        self.back[ipost].insert(ipre);
        // TODO resolve too much copying, can't prove that ipre isn't in to_add_back.
        // a dense matrix would improve things but then adding tiles stinks

        // tpost's descendants get backward links to all of tpre's ancestors
        for to_add_back in self.fwd[ipost].iter() {
            self.back[*to_add_back].insert(ipre);
            let cp: Vec<_> = self.back[ipre].iter().copied().collect();
            self.back[*to_add_back].extend(cp);
        }
        // tpre's ancestors get forward links to all of tpost's descendants
        for to_add_fwd in self.back[ipre].iter() {
            self.fwd[*to_add_fwd].insert(ipost);
            let cp: Vec<_> = self.fwd[ipost].iter().copied().collect();
            self.fwd[*to_add_fwd].extend(cp);
        }
    }

    fn goes_to(&self, tpre: Id<T>, tpost: Id<T>) -> bool {
        self.fwd[tpre.index()].contains(&(tpost.index()))
    }
}

pub type TileChange = Id<TileChangeData>;
impl Tile for TileChange {}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct TileChangeData {
    pub from: TileGfxId,
    pub to: TileGfxId,
    successors: Vec<(TileGfxId, usize)>,
    count: usize,
}

pub struct TileDB {
    gfx_arena: Arena<TileGfx>,
    change_arena: Arena<TileChangeData>,
    initial: TileGfxId,
    initial_change: TileChange,

    // TODO consider trie based on pixel runs?
    gfx: HashMap<TileGfx, TileGfxId>,

    changes: HashMap<(TileGfxId, TileGfxId), TileChange>,
    // change_closure: Chain<TileChange>,
}

impl TileDB {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut gfx_arena = Arena::new();
        let mut change_arena = Arena::new();
        let init_tile = TileGfx::new();
        let initial = gfx_arena.alloc(init_tile);
        let gfx = HashMap::new();
        let initial_change_data = TileChangeData {
            from: initial,
            to: initial,
            successors: vec![],
            count: 0,
        };
        let initial_change = change_arena.alloc(initial_change_data);
        let mut changes = HashMap::new();
        changes.insert((initial, initial), initial_change);
        // let mut change_closure = Chain::new();
        // change_closure.insert(initial_change);
        TileDB {
            gfx_arena,
            initial,
            gfx,
            change_arena,
            initial_change,
            changes,
            // change_closure,
        }
    }
    pub fn get_initial_change(&self) -> TileChange {
        self.initial_change
    }
    pub fn get_initial_tile(&self) -> TileGfxId {
        self.initial
    }
    pub fn get_tile(&mut self, tg: TileGfx) -> TileGfxId {
        let arena = &mut self.gfx_arena;
        let id = *self.gfx.entry(tg).or_insert_with(|| arena.alloc(tg));
        id
    }
    pub fn contains(&self, tg: &TileGfx) -> bool {
        self.gfx.contains_key(tg)
    }
    pub fn insert(&mut self, tile: TileGfx) {
        self.get_tile(tile);
    }
    pub fn extend<I>(&mut self, tgs: I)
    where
        I: IntoIterator<Item = TileGfx>,
    {
        tgs.into_iter().for_each(|tg| {
            self.get_tile(tg);
        });
    }
    pub fn get_tile_by_id(&self, tg: TileGfxId) -> Option<&TileGfx> {
        self.gfx_arena.get(tg)
    }
    pub fn get_change_by_id(&self, tc: TileChange) -> Option<&TileChangeData> {
        self.change_arena.get(tc)
    }
    pub fn gfx_iter(&self) -> impl Iterator<Item = &TileGfx> {
        self.gfx_arena.iter().map(|(_id, t)| t)
    }
    pub fn gfx_count(&self) -> usize {
        self.gfx.len()
    }
    pub fn change_cost(&self, tc1: TileChange, tc2: TileChange) -> f32 {
        let tc1_c = self.change_arena.get(tc1).unwrap();
        let tc2_c = self.change_arena.get(tc2).unwrap();
        if tc1 == tc2 || tc1 == self.initial_change || tc2 == self.initial_change {
            0.0
        } else if tc1_c.to == tc2_c.from || tc1_c.from == tc2_c.to {
            0.25
        }
        // else if self.changes.contains_key(&(tc1_c.to,tc2_c.from)) || self.changes.contains_key(&(tc2_c.to,tc1_c.from)) { //self.change_closure.goes_to(tc1, tc2) || self.change_closure.goes_to(tc2, tc1) {
        //     0.1
        // }
        else {
            1.0
        }
    }
    pub fn change_from_to(&mut self, tc: TileChange, gfx: TileGfxId) -> TileChange {
        let old_to = self.change_arena.get(tc).unwrap().to;
        if gfx == old_to {
            tc
        } else {
            // Note! Could change from not-initial to initial under some circumstances (sprites?)
            // Or if we go from a large region screen to a small region screen?
            // For now, just ignore
            if gfx == self.get_initial_tile() {
                return tc;
            }
            let arena = &mut self.change_arena;
            let tc2 = *self.changes.entry((old_to, gfx)).or_insert_with(|| {
                arena.alloc(TileChangeData {
                    from: old_to,
                    to: gfx,
                    successors: vec![],
                    count: 0,
                })
            });
            // self.change_closure.insert(tc2);

            // self.change_closure.chain(tc, tc2);
            let init = self.get_initial_change();
            let old_change = self.change_arena.get_mut(tc).unwrap();
            if tc != init {
                (*old_change).count -= 1;
            }
            let mut found = false;
            for (change_to, count) in (*old_change).successors.iter_mut() {
                if *change_to == gfx {
                    found = true;
                    *count += 1;
                    break;
                }
            }
            if !found {
                (*old_change).successors.push((gfx, 1));
            }

            self.change_arena.get_mut(tc2).unwrap().count += 1;
            tc2
        }
    }
    pub fn tile_stats(&self) -> TileDBStats {
        TileDBStats{
            gfx:self.gfx.len(),
            changes:self.changes.len(),
            // closure_sizes_fwd:
            // self.change_closure.fwd.iter().map(|c| c.len()).collect(),
            // closure_sizes_back:
            // self.change_closure.back.iter().map(|c| c.len()).collect(),
        }
    }
}

#[derive(Debug)]
pub struct TileDBStats {
    pub gfx:usize,
    pub changes:usize,
    // pub closure_sizes_fwd:Vec<usize>,
    // pub closure_sizes_back:Vec<usize>,
}
