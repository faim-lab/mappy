use crate::framebuffer::Framebuffer;
use retro_rs::pixels;
use std::hash::{Hash, Hasher};
use std::fmt;
use std::collections::HashMap;
use id_arena::{Arena, Id};

pub trait Tile : PartialEq + Eq + Hash + Clone {

}


#[derive(Clone, Copy)]
pub struct TileGfx(pub [u8; 8 * 8]);

impl TileGfx {
    pub fn read(fb: &Framebuffer, x: usize, y: usize) -> Self {
        let mut tile_data = [0_u8; 64];
        for yi in y..y + 8 {
            for xi in x..x + 8 {
                tile_data[(((yi - y) as u8) * 8 + ((xi - x) as u8)) as usize] =
                    fb.fb[fb.w * yi + xi];
            }
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
        self.0.iter().fold(0_u128, |x,&y| x.wrapping_add(y as u128))
    }
    pub fn new() -> Self {
        Self([0;8*8])
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

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TileChange {
    pub from:TileGfxId,
    pub to:TileGfxId
}
impl TileChange {
    pub fn new(from:TileGfxId, to:TileGfxId) -> Self {
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
    successors:Vec<(TileGfxId, usize)>,
    count: usize
}

pub struct TileDB {
    gfx_arena: Arena<TileGfx>,
    initial: TileGfxId,

    // TODO consider trie based on pixel runs?
    gfx: HashMap<TileGfx, TileGfxId>,

    changes: HashMap<TileChange, TileChangeData>
}

impl TileDB {
    pub fn new() -> Self {
        let mut gfx_arena = Arena::new();
        let init_tile = TileGfx::new();
        let initial = gfx_arena.alloc(init_tile);
        let gfx = HashMap::new();
        let mut changes = HashMap::new();
        changes.insert(TileChange::new(initial,initial), TileChangeData::default());
        TileDB {
            gfx_arena,
            initial,
            gfx,
            changes
        }
    }
    pub fn get_initial_change(&self) -> TileChange {
        TileChange{from:self.initial, to:self.initial}
    }
    pub fn get_initial_tile(&self) -> TileGfxId {
        self.initial
    }
    pub fn get_tile(&mut self, tg:TileGfx) -> TileGfxId {
        let arena = &mut self.gfx_arena;
        *self.gfx.entry(tg).or_insert_with(|| {
            arena.alloc(tg)
        })
    }
    pub fn contains(&self, tg:&TileGfx) -> bool {
        self.gfx.contains_key(tg)
    }
    pub fn extend<I>(&mut self, tgs:I) where
        I:IntoIterator<Item=TileGfx> {
        tgs.into_iter().for_each(|tg| { self.get_tile(tg); });
    }
    pub fn get_tile_by_id(&self, tg:TileGfxId) -> Option<&TileGfx> {
        self.gfx_arena.get(tg)
    }
    pub fn gfx_iter(&self) -> impl Iterator<Item=&TileGfx> {
        self.gfx.keys()
    }
    pub fn gfx_count(&self) -> usize {
        self.gfx.len()
    }
    pub fn change_from_to(&mut self, tc:&TileChange, gfx:&TileGfxId) -> TileChange {
        if gfx == &tc.to { tc.clone() }
        else {
            // Note! Could change from not-initial to initial under some circumstances (sprites?)
            // Or if we go from a large region screen to a small region screen?
            // For now, just ignore
            if gfx == &self.get_initial_tile() {
                return tc.clone()
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
            let change = self.changes.entry(tc2.clone()).or_insert(TileChangeData::default());
            change.count += 1;
            tc2
        }
    }
}
