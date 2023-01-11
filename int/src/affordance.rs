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
struct ModulateSettings {
    avatar_ratio: f32,
    dangerous_ratio: f32,
    usable_ratio: f32,
    portal_ratio: f32,
    changeable_ratio: f32,
    breakable_ratio: f32,
    solid_ratio: f32,
    movable_saturation_change: f32,
    no_affordance_saturation_change: f32,
}
pub struct AffordanceTracker {
    // later: output path, etc, like scroll dumper
    tiles: HashMap<u128, Affordance>,
    sprites: HashMap<u32, Affordance>,

    brush: AffordanceMask,
    settings: ModulateSettings,
}
impl AffordanceTracker {
    pub fn new(_romname: &str) -> Self {
        Self {
            tiles: HashMap::with_capacity(10_000),
            sprites: HashMap::with_capacity(10_000),
            brush: AffordanceMask::empty(),
            settings: ModulateSettings {
                avatar_ratio: 1.0,
                dangerous_ratio: 1.0,
                usable_ratio: 1.0,
                portal_ratio: 1.0,
                changeable_ratio: 1.0,
                breakable_ratio: 1.0,
                solid_ratio: 1.0,
                movable_saturation_change: 2.0,
                no_affordance_saturation_change: 0.5,
            },
        }
    }
    fn draw_brush_display(&self) {
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
    }
    fn update_brush(&mut self) {
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
    }
    pub fn update(&mut self, mappy: &MappyState, _emu: &Emulator) {
        self.update_brush();
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
    #[allow(clippy::map_entry)]
    pub fn modulate(
        &mut self,
        mappy: &MappyState,
        _emu: &Emulator,
        in_img: &Image,
        out_img: &mut Image,
    ) {
        //Rendering: for now, desaturate/reduce contrast of ones with no affordances, tint danger red, make solid high contrast, make avatar green, tint usable/breakable/portal/etc blue.
        let tiles = mappy.tiles.read().unwrap();
        let region = mappy.split_region();
        let sr = mappy.current_screen.region;
        use image::{GenericImage, GenericImageView};
        use imageproc::{drawing as d, rect::Rect};
        let in_img: image::ImageBuffer<image::Rgba<u8>, &[u8]> = image::ImageBuffer::from_raw(
            in_img.width as u32,
            in_img.height as u32,
            in_img.bytes.as_slice(),
        )
        .unwrap();
        let mut out_img: image::ImageBuffer<image::Rgba<u8>, &mut [u8]> =
            image::ImageBuffer::from_raw(
                out_img.width as u32,
                out_img.height as u32,
                out_img.bytes.as_mut_slice(),
            )
            .unwrap();
        out_img.copy_from(&in_img, 0, 0).unwrap();
        let mut canvas = d::Blend(out_img);
        for x in ((region.x)..(region.x + region.w as i32)).step_by(TILE_SIZE) {
            for y in ((region.y)..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                if let Some(gfx) = mappy
                    .current_room
                    .as_ref()
                    .and_then(|r| {
                        r.get(
                            sr.x + (x - region.x) / TILE_SIZE as i32,
                            sr.y + (y - region.y) / TILE_SIZE as i32,
                        )
                    })
                    .and_then(|tile| tiles.get_change_by_id(tile))
                    .and_then(|change| tiles.get_tile_by_id(change.to))
                {
                    match self.tiles.get(&gfx.perceptual_hash()) {
                        None => {
                            // todo, highlight un-known nature
                        }
                        Some(Affordance::Guessed(mask) | Affordance::Given(mask)) => {
                            apply_mask_to_area(
                                &mut canvas,
                                *mask,
                                x as u32,
                                y as u32,
                                TILE_SIZE as u32,
                                TILE_SIZE as u32,
                                &self.settings,
                            );
                        }
                    }
                }
            }
        }
        //let initial_tile = mappy.tiles.read().unwrap().get_initial_tile();
        for track in mappy.live_tracks.iter() {
            let cur = track.current_data();
            // if every tile covered by this sprite is a known tile in the current _screen_, skip it
            // TODO: later: mask out individual pixels of the sprite
            // let sprite_is_clear = tiles_covered_by(mappy, cur)
            //     .into_iter()
            //     .all(|(tx, ty)| mappy.current_screen.get(tx, ty) != Some(initial_tile));
            // if sprite_is_clear {
            //     continue;
            // }
            if !self.sprites.contains_key(&cur.key()) {
                if let Some(guess) = sprite_guesses(mappy, track).fold(None, |guess, track_key| {
                    match (self.sprites.get(&track_key), guess) {
                        (None, guess) => guess,
                        (Some(guess), None) => Some(*guess),
                        (Some(Affordance::Given(mask)), _old) => Some(Affordance::Given(*mask)),
                        (Some(Affordance::Guessed(_mask)), better_guess) => better_guess,
                    }
                }) {
                    self.sprites.insert(
                        cur.key(),
                        Affordance::Guessed(match guess {
                            Affordance::Given(mask) => mask,
                            Affordance::Guessed(mask) => mask,
                        }),
                    );
                }
            }
            let mappy::sprites::At(_, _, sd) = track.positions.last().unwrap();
            if sd.x as u32 + sd.width() as u32 > 255 || sd.y as u32 + sd.height() as u32 > 240 {
                continue;
            }
            canvas
                .0
                .copy_from(
                    &*in_img.view(
                        sd.x as u32,
                        sd.y as u32,
                        sd.width() as u32,
                        sd.height() as u32,
                    ),
                    sd.x as u32,
                    sd.y as u32,
                )
                .unwrap();
            match self.sprites.get(&cur.key()) {
                None => {
                    // todo, highlight unknown nature
                }
                Some(Affordance::Guessed(mask) | Affordance::Given(mask)) => {
                    if track.get_is_avatar() {
                        emphasize(
                            &mut canvas,
                            Rect::at(sd.x as i32, sd.y as i32)
                                .of_size(sd.width() as u32, sd.height() as u32),
                            image::Rgba([0, 255, 0, 255]),
                            self.settings.avatar_ratio,
                        );
                    } else {
                        apply_mask_to_area(
                            &mut canvas,
                            *mask,
                            sd.x as u32,
                            sd.y as u32,
                            sd.width() as u32,
                            sd.height() as u32,
                            &self.settings,
                        );
                    }
                }
            }
        }
        self.draw_brush_display();
    }
}

fn apply_mask_to_area<I: image::GenericImage<Pixel = image::Rgba<u8>>>(
    canvas: &mut imageproc::drawing::Blend<I>,
    mask: AffordanceMask,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    settings: &ModulateSettings,
) {
    use imageproc::rect::Rect;
    let r = Rect::at(x as i32, y as i32).of_size(w, h);
    if mask.contains(AffordanceMask::DANGER) {
        emphasize(
            canvas,
            r,
            image::Rgba([255, 0, 0, 255]),
            settings.dangerous_ratio,
        );
    } else if mask.contains(AffordanceMask::USABLE) {
        emphasize(
            canvas,
            r,
            image::Rgba([255, 255, 0, 255]),
            settings.usable_ratio,
        );
    } else if mask.contains(AffordanceMask::PORTAL) {
        emphasize(
            canvas,
            r,
            image::Rgba([0, 0, 255, 255]),
            settings.portal_ratio,
        );
    } else if mask.contains(AffordanceMask::CHANGEABLE) {
        emphasize(
            canvas,
            r,
            image::Rgba([150, 75, 0, 255]),
            settings.changeable_ratio,
        );
    } else if mask.contains(AffordanceMask::BREAKABLE) {
        emphasize(
            canvas,
            r,
            image::Rgba([150, 75, 0, 255]),
            settings.breakable_ratio,
        );
    } else if mask.contains(AffordanceMask::SOLID) {
        emphasize(
            canvas,
            r,
            image::Rgba([196, 196, 196, 255]),
            settings.solid_ratio,
        );
    } else if mask.contains(AffordanceMask::MOVABLE) {
        emphasize_saturation(canvas, r, settings.movable_saturation_change);
    } else {
        emphasize_saturation(canvas, r, settings.no_affordance_saturation_change);
    }
}

fn emphasize<I: image::GenericImage<Pixel = image::Rgba<u8>>>(
    canvas: &mut imageproc::drawing::Blend<I>,
    r: imageproc::rect::Rect,
    target: image::Rgba<u8>,
    ratio: f32,
) {
    // TODO: compute HSV of r in canvas, modulate each color towards target by ratio
    // can't do a lerp exactly, or can I?
    // what if I literally did a lerp in RGB and then blended the new and old pixels by ratio?
    imageproc::drawing::draw_filled_rect_mut(canvas, r, target);
}

fn emphasize_saturation<I: image::GenericImage<Pixel = image::Rgba<u8>>>(
    canvas: &mut imageproc::drawing::Blend<I>,
    r: imageproc::rect::Rect,
    change_by: f32,
) {
    // TODO: compute saturation of r in canvas, multiply by change_by, draw that into area; or use a saturate() command if it exists
    imageproc::drawing::draw_filled_rect_mut(canvas, r, image::Rgba([255, 255, 255, 64]));
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
#[allow(dead_code)]
fn tiles_covered_by(mappy: &MappyState, cur: &mappy::sprites::SpriteData) -> [(i32, i32); 4] {
    [
        mappy.screen_to_tile(cur.x as i32, cur.y as i32),
        mappy.screen_to_tile(cur.x as i32 + cur.width() as i32 - 1, cur.y as i32),
        mappy.screen_to_tile(cur.x as i32, cur.y as i32 + cur.height() as i32 - 1),
        mappy.screen_to_tile(
            cur.x as i32 + cur.width() as i32 - 1,
            cur.y as i32 + cur.height() as i32 - 1,
        ),
    ]
}
