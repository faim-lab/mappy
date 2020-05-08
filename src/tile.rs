use crate::framebuffer::Framebuffer;
use crate::pixels;
use std::hash::{Hash, Hasher};
use std::fmt;
use std::rc::Rc;

pub trait Tile : PartialEq + Eq + Hash + Clone {
    fn empty() -> Self;
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
        for yi in 0..8 {
            for xi in 0..8 {
                let col = self.0[yi * 8 + xi];
                let (r, g, b) = pixels::rgb332_to_rgb888(col);
                buf[(yi * 8 + xi) * 3] = r;
                buf[(yi * 8 + xi) * 3 + 1] = g;
                buf[(yi * 8 + xi) * 3 + 2] = b;
            }
        }
    }
    pub fn perceptual_hash(&self) -> u128 {
        self.0.iter().fold(0_u128, |x,&y| x.wrapping_add(y as u128))
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
impl Tile for TileGfx {
    fn empty() -> Self {
        TileGfx([0;8*8])
    }
}
impl fmt::Debug for TileGfx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileGfx")
            .field("phash", &self.perceptual_hash())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TileAnim {
    pub frames:Vec<TileGfx>
}
impl TileAnim {
    pub fn new(gfx:TileGfx) -> Self {
        TileAnim {
            frames:vec![gfx]
        }
    }
    pub fn extend(&self, gfx:TileGfx) -> Self {
        TileAnim {
            frames:self.frames.iter().cloned().chain(vec![gfx]).collect()
        }
    }
}
impl fmt::Debug for TileAnim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TileAnim")
            .field("phashes", &self.frames.iter().map(|f| f.perceptual_hash()).collect::<Vec<_>>())
            .finish()
    }
}
impl Tile for TileAnim {
    fn empty() -> Self {
        TileAnim{
            frames:vec![]
        }
    }
}
impl<T> Tile for Rc<T> where T:Tile {
    fn empty() -> Self {
        Rc::new(T::empty())
    }
}

// use std::collections::HashMap;

// // TODO find a lighter weight alternative to TileGfx
// pub struct AnimTrie {
//     nodes:HashMap<TileGfx, AnimTrieNode>
// }

// impl AnimTrie {
//     pub fn new() -> Self {
//         Self { nodes:HashMap::new()}
//     }
//     pub fn lookup_ext(&self, ta:&TileAnim, gfx:&TileGfx) -> Option<Rc<TileAnim>> {
//         if let Some(n) = self.nodes.get(&ta.frames[0]) {
//             return n.lookup_ext(ta, 1, gfx);
//         }
//         None
//     }
//     pub fn insert(&mut self, ta:Rc<TileAnim>) {
//         let gfx = ta.frames[0];
//         use std::collections::hash_map::Entry;
//         match self.nodes.entry(ta.frames[0]) {
//             Entry::Occupied(o) => {
//                 o.get().insert(ta, 1);
//             }
//             Entry::Vacant(v) => {
//                 assert!(ta.frames.len() == 1);
//                 v.insert(AnimTrieNode {
//                     value:ta,
//                     nodes:HashMap::new()
//                 });
//             }
//         };
//     }
// }

// struct AnimTrieNode {
//     value:Rc<TileAnim>,
//     nodes:HashMap<TileGfx, AnimTrieNode>
// }

// impl AnimTrieNode {
//     pub fn insert(&mut self, ta:Rc<TileAnim>, idx:usize) {
//         if idx > ta.frames.len() {
//             panic!("Bug in AnimTrieNode");
//         } else if idx == ta.frames.len() {
//             assert_eq!(ta, self.value);
//         } else if idx == ta.frames.len() - 1 {
//             let gfx = ta.frames[idx];
//             self.nodes.entry(gfx).or_insert_with(|| {
//                 AnimTrieNode {value:ta.clone(), nodes:HashMap::new()}
//             });
//         } else {
//             let gfx = &ta.frames[idx];
//             let next = self.nodes.get(gfx).expect("Inserted a bigger animation before its smaller component!");
//             next.insert(ta, idx+1);
//         }
//     }
//     pub fn lookup_ext(&self, ta:&TileAnim, idx:usize, ext_gfx:&TileGfx) -> Option<Rc<TileAnim>> {
//         if idx == ta.frames.len() + 1 {
//             Some(self.value)
//         } else if idx > ta.frames.len() {
//             panic!("Bad tileanim lookup in AnimTrieNode");
//         } else {
//             let gfx = if idx < ta.frames.len() { &ta.frames[idx] } else { ext_gfx };
//             match self.nodes.get(gfx) {
//                 Some(n) => n.lookup_ext(ta, idx+1, ext_gfx),
//                 None => None
//             }
//         }
//     }
// }
