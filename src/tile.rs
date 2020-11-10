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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileChange {
    pub from: TileGfxId,
    pub to: TileGfxId,
}
impl TileChange {
    pub fn new(from: TileGfxId, to: TileGfxId) -> Self {
        TileChange { from, to }
    }
}
impl fmt::Debug for TileChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileChange")
            .field("from", &self.from)
            .field("to", &self.to)
            .finish()
    }
}
impl Tile for TileChange {}

#[derive(Default)]
struct TileChangeData {
    successors: Vec<(TileGfxId, usize)>,
    count: usize,
}

pub struct TileDB {
    gfx_arena: Arena<TileGfx>,
    initial: TileGfxId,

    // TODO consider trie based on pixel runs?
    gfx: HashMap<TileGfx, TileGfxId>,

    changes: HashMap<TileChange, TileChangeData>,
}

impl TileDB {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut gfx_arena = Arena::new();
        let init_tile = TileGfx::new();
        let initial = gfx_arena.alloc(init_tile);
        let gfx = HashMap::new();
        let mut changes = HashMap::new();
        changes.insert(TileChange::new(initial, initial), TileChangeData::default());
        TileDB {
            gfx_arena,
            initial,
            gfx,
            changes,
        }
    }
    pub fn get_initial_change(&self) -> TileChange {
        TileChange {
            from: self.initial,
            to: self.initial,
        }
    }
    pub fn get_initial_tile(&self) -> TileGfxId {
        self.initial
    }
    pub fn get_tile(&mut self, tg: TileGfx) -> TileGfxId {
        let arena = &mut self.gfx_arena;
        *self.gfx.entry(tg).or_insert_with(|| arena.alloc(tg))
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
    pub fn gfx_iter(&self) -> impl Iterator<Item = &TileGfx> {
        self.gfx.keys()
    }
    pub fn gfx_count(&self) -> usize {
        self.gfx.len()
    }
    pub fn change_from_to(&mut self, tc: &TileChange, gfx: &TileGfxId) -> TileChange {
        if gfx == &tc.to {
            *tc
        } else {
            // Note! Could change from not-initial to initial under some circumstances (sprites?)
            // Or if we go from a large region screen to a small region screen?
            // For now, just ignore
            if gfx == &self.get_initial_tile() {
                return *tc;
            }
            let init = self.get_initial_change();
            let old_change = self.changes.get_mut(&tc).unwrap();
            if *tc != init {
                (*old_change).count -= 1;
            }
            let mut found = false;
            for (change_to, count) in (*old_change).successors.iter_mut() {
                if change_to == gfx {
                    found = true;
                    *count += 1;
                    break;
                }
            }
            if !found {
                (*old_change).successors.push((*gfx, 1));
            }

            let tc2 = TileChange::new(tc.to, *gfx);
            let change = self
                .changes
                .entry(tc2)
                .or_insert_with(TileChangeData::default);
            change.count += 1;
            tc2
        }
    }
}
