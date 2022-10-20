use bitflags::bitflags;
use macroquad::prelude::*;
use mappy::{sprites::SpriteTrack, MappyState, TILE_SIZE};
use retro_rs::Emulator;
use std::collections::{HashMap, HashSet};
bitflags! {
    struct AffordanceMask : u8 {
        const SOLID      = 0b0000_0000_0000_0001;
        const DANGER     = 0b0000_0000_0000_0010;
        const CHANGEABLE = 0b0000_0000_0000_0100;
        const USABLE     = 0b0000_0000_0000_1000;
        const PORTAL     = 0b0000_0000_0001_0000;
        const MOVABLE    = 0b0000_0000_0010_0000;
        const BREAKABLE  = 0b0000_0000_0100_0000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Affordance {
    Guessed(AffordanceMask),
    Given(AffordanceMask),
}
enum Action {
    Paste,     // propagates a guess which also clobbers other guesses
    Cut(bool), // false is copy, true is erase; erase clobbers guesses
}
pub struct AffordanceTracker {
    // later: output path, etc, like scroll dumper
    tiles: HashMap<u128, Affordance>,
    sprites: HashMap<u32, Affordance>,

    brush: AffordanceMask,
}
impl AffordanceTracker {
    pub fn new(_romname: &str) -> Self {
        Self {
            tiles: HashMap::with_capacity(10_000),
            sprites: HashMap::with_capacity(10_000),
            brush: AffordanceMask::empty(),
        }
    }
    pub fn update(&mut self, mappy: &MappyState, _emu: &Emulator) {
        // update brush
        if is_key_pressed(KeyCode::Kp7) {
            self.brush.toggle(AffordanceMask::SOLID);
        }
        if is_key_pressed(KeyCode::Kp8) {
            self.brush.toggle(AffordanceMask::DANGER);
        }
        if is_key_pressed(KeyCode::Kp9) {
            self.brush.toggle(AffordanceMask::CHANGEABLE);
        }
        if is_key_pressed(KeyCode::Kp4) {
            self.brush.toggle(AffordanceMask::USABLE);
        }
        if is_key_pressed(KeyCode::Kp5) {
            self.brush.toggle(AffordanceMask::PORTAL);
        }
        if is_key_pressed(KeyCode::Kp6) {
            self.brush.toggle(AffordanceMask::MOVABLE);
        }
        if is_key_pressed(KeyCode::Kp1) {
            self.brush.toggle(AffordanceMask::BREAKABLE);
        }
        draw_text(
            &format!(
                "{}{}{}\n{}{}{}\n{}",
                if self.brush.contains(AffordanceMask::SOLID) {
                    "S"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::DANGER) {
                    "D"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::CHANGEABLE) {
                    "C"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::USABLE) {
                    "U"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::PORTAL) {
                    "P"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::MOVABLE) {
                    "M"
                } else {
                    " "
                },
                if self.brush.contains(AffordanceMask::BREAKABLE) {
                    "B"
                } else {
                    " "
                },
            ),
            4.0 * super::SCALE,
            260.0 * super::SCALE,
            super::SCALE * 24.0,
            RED,
        );
        draw_text(
            &format!(
                "{}{}{}\n{}{}{}\n{}",
                if self.brush.contains(AffordanceMask::SOLID) {
                    " "
                } else {
                    "S"
                },
                if self.brush.contains(AffordanceMask::DANGER) {
                    " "
                } else {
                    "D"
                },
                if self.brush.contains(AffordanceMask::CHANGEABLE) {
                    " "
                } else {
                    "C"
                },
                if self.brush.contains(AffordanceMask::USABLE) {
                    " "
                } else {
                    "U"
                },
                if self.brush.contains(AffordanceMask::PORTAL) {
                    " "
                } else {
                    "P"
                },
                if self.brush.contains(AffordanceMask::MOVABLE) {
                    " "
                } else {
                    "M"
                },
                if self.brush.contains(AffordanceMask::BREAKABLE) {
                    " "
                } else {
                    "B"
                },
            ),
            4.0 * super::SCALE,
            260.0 * super::SCALE,
            super::SCALE * 24.0,
            GRAY,
        );
        // left click to grant, right click to copy affordances
        // shift right click to cut and erase (propagating to guesses)
        let action = match (
            is_mouse_button_down(MouseButton::Left),
            is_mouse_button_down(MouseButton::Right),
            is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift),
        ) {
            (true, _, _) => Some(Action::Paste),
            (false, true, shifted) => Some(Action::Cut(shifted)),
            (_, _, _) => None,
        };
        let (mx, my) = mouse_position();
        let sprite = mappy.live_tracks.iter().find(|track| {
            mappy::sprites::overlapping_sprite(
                (mx / super::SCALE) as usize,
                (my / super::SCALE) as usize,
                2,
                2,
                &[*track.current_data()],
            )
        });
        let tile = if sprite.is_none() {
            let (tx, ty) = super::screen_f32_to_tile((mx, my), mappy);
            mappy
                .current_room
                .as_ref()
                .and_then(|room| room.get(tx, ty))
                .and_then(|change| {
                    let tiles = mappy.tiles.read().unwrap();
                    let change_data = tiles.get_change_by_id(change);
                    if let Some(cd) = change_data {
                        let to = cd.to;
                        tiles.get_tile_by_id(to).map(|t| t.perceptual_hash())
                    } else {
                        None
                    }
                })
        } else {
            None
        };
        match action {
            None => {}
            Some(Action::Paste) => {
                match (sprite, tile) {
                    (Some(track), _) => {
                        // add given if not present, else upgrade to given if present
                        let main_key = track.current_data().key();
                        self.sprites.insert(main_key, Affordance::Given(self.brush));
                        for key in sprite_guesses(mappy, track) {
                            if key != main_key {
                                use std::collections::hash_map::Entry;
                                match self.sprites.entry(key) {
                                    Entry::Vacant(v) => {
                                        v.insert(Affordance::Guessed(self.brush));
                                    }
                                    Entry::Occupied(mut v) => {
                                        if let Affordance::Guessed(_g) = v.get() {
                                            v.insert(Affordance::Guessed(self.brush));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (None, Some(tile_hash)) => {
                        self.tiles.insert(tile_hash, Affordance::Given(self.brush));
                        // no propagation for tiles
                        // in the future, could do something like "same pattern, different palette" or "all tiles which were in the same position in the world/part of the same tile animation"
                    }
                    (None, None) => {}
                }
            }
            Some(Action::Cut(and_delete)) => match (sprite, tile) {
                (Some(track), _) => {
                    // if there is no guess to cut, do nothing
                    let key = track.current_data().key();
                    self.brush = match self.sprites.get(&key) {
                        Some(Affordance::Guessed(b)) => *b,
                        Some(Affordance::Given(b)) => *b,
                        None => self.brush,
                    };
                    if and_delete {
                        self.sprites.remove(&key);
                        for g in sprite_guesses(mappy, track) {
                            if g != key {
                                if let Some(Affordance::Guessed(_guess)) = self.sprites.get(&g) {
                                    self.sprites.remove(&g);
                                }
                            }
                        }
                    }
                }
                (None, Some(tile_hash)) => {
                    self.brush = match self.tiles.get(&tile_hash) {
                        Some(Affordance::Guessed(b)) => *b,
                        Some(Affordance::Given(b)) => *b,
                        None => self.brush,
                    };
                    if and_delete {
                        self.tiles.remove(&tile_hash);
                        // no propagation for tiles
                        // in the future, could do something like "same pattern, different palette"
                    }
                }
                (None, None) => {}
            },
        }
    }
    pub fn modulate(&mut self, mappy: &MappyState, _emu: &Emulator, img: &mut Image) {
        //Rendering: for now, desaturate/reduce contrast of ones with no affordances, tint danger red, make solid high contrast, make avatar green, tint usable/breakable/portal/etc blue.
        let tiles = mappy.tiles.read().unwrap();
        let region = mappy.split_region();
        let sr = mappy.current_screen.region;
        for x in ((region.x)..(region.x + region.w as i32)).step_by(TILE_SIZE) {
            for y in ((region.y)..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                if let Some(gfx) = mappy
                    .current_screen
                    .get(
                        sr.x + (x - region.x) / TILE_SIZE as i32,
                        sr.y + (y - region.y) / TILE_SIZE as i32,
                    )
                    .and_then(|tile| tiles.get_tile_by_id(tile))
                {
                    match self.tiles.get(&gfx.perceptual_hash()) {
                        None => {
                            // todo, highlight un-known nature
                        }
                        Some(Affordance::Guessed(mask) | Affordance::Given(mask)) => {
                            let mask = (mask.bits() | 0x000F) as u32;
                            draw_rectangle(
                                (x as f32 * super::SCALE) as f32,
                                (y as f32 * super::SCALE) as f32,
                                TILE_SIZE as f32 * super::SCALE,
                                TILE_SIZE as f32 * super::SCALE,
                                Color::new(
                                    (mask * 127 % 256) as f32 / 255.,
                                    (mask * 33 % 256) as f32 / 255.,
                                    (mask * 61 % 256) as f32 / 255.,
                                    0.5,
                                ),
                            );
                        }
                    }
                }
            }
        }
        for track in mappy.live_tracks.iter() {
            let cur = track.current_data();
            match self.sprites.get(&cur.key()) {
                None => {
                    // todo, highlight unknown nature
                }
                Some(Affordance::Guessed(mask) | Affordance::Given(mask)) => {
                    let mask = (mask.bits() | 0x000F) as u32;

                    let col = Color::new(
                        (mask * 127 % 256) as f32 / 255.,
                        (mask * 33 % 256) as f32 / 255.,
                        (mask * 61 % 256) as f32 / 255.,
                        0.5,
                    );
                    let mappy::sprites::At(_, _, sd) = track.positions.last().unwrap();
                    draw_rectangle(
                        sd.x as f32 * super::SCALE,
                        sd.y as f32 * super::SCALE,
                        sd.width() as f32 * super::SCALE,
                        sd.height() as f32 * super::SCALE,
                        2.0,
                        col,
                    );
                }
            }
        }
    }
}

fn sprite_guesses(mappy: &MappyState, track: &SpriteTrack) -> impl Iterator<Item = u32> {
    // all sprites on every track of every blob including track
    let mut set = HashSet::new();

    for d in track.positions.iter() {
        set.insert(d.2.key());
    }
    for b in mappy
        .live_blobs
        .iter()
        .filter(|b| b.contains_live_track(track.id))
    {
        for t in b.live_tracks.iter() {
            for d in mappy.live_track_with_id(t).unwrap().positions.iter() {
                set.insert(d.2.key());
            }
        }
    }
    set.into_iter()
}
