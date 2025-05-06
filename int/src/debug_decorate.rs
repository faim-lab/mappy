#[allow(clippy::wildcard_imports)]
use super::*;

pub struct Decorator {
    pub deco: Box<dyn Deco>,
    pub enabled: bool,
    pub toggle: KeyCode,
}

pub trait Deco {
    fn draw(&mut self, mappy: &MappyState);
}

pub struct Grid {}
impl Deco for Grid {
    #[allow(clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn draw(&mut self, mappy: &MappyState) {
        let region = mappy.split_region();
        for x in ((region.x as u32)..(region.x as u32 + region.w)).step_by(TILE_SIZE) {
            draw_line(
                x as f32 * SCALE,
                SCALE * region.y as f32,
                x as f32 * SCALE,
                SCALE * (region.y as f32 + region.h as f32),
                1.,
                RED,
            );
        }
        for y in ((region.y as u32)..(region.y as u32 + region.h)).step_by(TILE_SIZE) {
            draw_line(
                SCALE * region.x as f32,
                y as f32 * SCALE,
                (SCALE) * (region.x as f32 + region.w as f32),
                y as f32 * SCALE,
                1.,
                RED,
            );
        }
    }
}
pub struct TileStandin {}
impl Deco for TileStandin {
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn draw(&mut self, mappy: &MappyState) {
        let region = mappy.split_region();
        let sr = mappy.current_screen.region;
        for x in ((region.x)..(region.x + region.w as i32)).step_by(TILE_SIZE) {
            for y in ((region.y)..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                // Use tile hash and convert to a 24-bit color
                let tile = mappy.current_screen[(
                    sr.x + (x - region.x) / TILE_SIZE as i32,
                    sr.y + (y - region.y) / TILE_SIZE as i32,
                )];
                let idx = tile.index();
                if idx != 0 {
                    // TODO this but better
                    draw_rectangle(
                        x as f32 * SCALE,
                        y as f32 * SCALE,
                        TILE_SIZE as f32 * SCALE,
                        TILE_SIZE as f32 * SCALE,
                        Color::new(
                            f32::from(idx * 127 % 256) / 255.,
                            f32::from(idx * 33 % 256) / 255.,
                            f32::from(idx * 61 % 256) / 255.,
                            1.,
                        ),
                    );
                }
            }
        }
    }
}
pub struct LiveTracks {
    pub dims: (usize, usize),
}
impl Deco for LiveTracks {
    #[allow(clippy::similar_names, clippy::cast_precision_loss)]
    fn draw(&mut self, mappy: &MappyState) {
        for track in &mappy.live_tracks {
            let col = Color::new(
                (*(track.positions[0].0) * 31 % 256) as f32 / 255.,
                (*(track.positions[0].0) * 127 % 256) as f32 / 255.,
                (*(track.positions[0].0) * 91 % 256) as f32 / 255.,
                1.,
            );
            let startp = Vec2::new(
                ((track.positions[0].1).0 + i32::from(track.positions[0].2.x) - mappy.scroll.0)
                    as f32,
                ((track.positions[0].1).1 + i32::from(track.positions[0].2.y) - mappy.scroll.1)
                    as f32,
            );
            draw_rectangle(
                SCALE * (startp.x.max(0.)).min(self.dims.0 as f32) - SCALE * 2.,
                SCALE * (startp.y.max(0.)).min(self.dims.1 as f32) - SCALE * 2.,
                SCALE * 4.,
                SCALE * 4.,
                col,
            );
            if track.positions.len() > 1 {
                for pair in track.positions.windows(2) {
                    let mappy::sprites::At(_, (sx0, sy0), sd0) = pair[0];
                    let x0 = sx0 + i32::from(sd0.x) - mappy.scroll.0;
                    let y0 = sy0 + i32::from(sd0.y) - mappy.scroll.1;
                    let mappy::sprites::At(_, (sx1, sy1), sd1) = pair[1];
                    let x1 = sx1 + i32::from(sd1.x) - mappy.scroll.0;
                    let y1 = sy1 + i32::from(sd1.y) - mappy.scroll.1;
                    draw_line(
                        x0 as f32 * SCALE,
                        y0 as f32 * SCALE,
                        x1 as f32 * SCALE,
                        y1 as f32 * SCALE,
                        1.,
                        col,
                    );
                }
                let mappy::sprites::At(_, _, sd) = track.positions.last().unwrap();
                draw_rectangle_lines(
                    f32::from(sd.x) * SCALE,
                    f32::from(sd.y) * SCALE,
                    f32::from(sd.width()) * SCALE,
                    f32::from(sd.height()) * SCALE,
                    2.0,
                    col,
                );
            }
        }
    }
}

pub struct LiveBlobs {}
impl Deco for LiveBlobs {
    #[allow(clippy::cast_precision_loss)]
    fn draw(&mut self, mappy: &MappyState) {
        for blob in &mappy.live_blobs {
            let col = Color::new(
                (*(blob.positions[0].0) * 31 % 256) as f32 / 255.,
                (*(blob.positions[0].0) * 127 % 256) as f32 / 255.,
                (*(blob.positions[0].0) * 91 % 256) as f32 / 255.,
                1.,
            );
            // let (_time, x, y) = blob.positions.last().unwrap();
            let (_time, bbox) = blob.bounding_boxes.last().unwrap();
            draw_rectangle_lines(
                (bbox.x.max(0) - mappy.scroll.0) as f32 * SCALE,
                (bbox.y.max(0) - mappy.scroll.1) as f32 * SCALE,
                bbox.w as f32 * SCALE,
                bbox.h as f32 * SCALE,
                2.0,
                col,
            );
        }
    }
}
pub struct Avatar {}
impl Deco for Avatar {
    #[allow(clippy::similar_names, clippy::cast_precision_loss)]
    fn draw(&mut self, mappy: &MappyState) {
        for track in &mappy.live_tracks {
            if track.get_is_avatar() {
                let mappy::sprites::At(_, (sx0, sy0), sd0) = track.positions.last().unwrap();
                let x0 = sx0 + i32::from(sd0.x) - mappy.scroll.0;
                let y0 = sy0 + i32::from(sd0.y) - mappy.scroll.1;
                draw_circle(x0 as f32 * SCALE, y0 as f32 * SCALE, 4.0, DARKBLUE);
            }
        }
    }
}
pub struct Recording {}
impl Deco for Recording {
    fn draw(&mut self, mappy: &MappyState) {
        if mappy.mapping {
            //draw a little red circle in the corner
            draw_circle(8.0 * SCALE, 8.0 * SCALE, 4.0 * SCALE, RED);
        }
    }
}

pub struct SelectedTile {
    pub selected_tile_pos: Option<(i32, i32)>,
}
impl Deco for SelectedTile {
    fn draw(&mut self, mappy: &MappyState) {
        if is_mouse_button_down(MouseButton::Left) && mappy.current_room.is_some() {
            let (tx, ty) = screen_f32_to_tile(mouse_position(), mappy);
            self.selected_tile_pos = Some((tx, ty));
        }
        if let Some((tx, ty)) = self.selected_tile_pos {
            let (sx, sy) = tile_to_screen((tx, ty), mappy);
            draw_rectangle_lines(sx, sy, 8.0 * SCALE, 8.0 * SCALE, 1.0 * SCALE, RED);
            if let Some(change) = mappy.current_room.as_ref().and_then(|r| r.get(tx, ty)) {
                let tiles = mappy.tiles.read().unwrap();
                let change_data = tiles.get_change_by_id(change);
                if let Some(cd) = change_data {
                    let to = cd.to;
                    let tile = tiles.get_tile_by_id(to).unwrap();
                    draw_text(
                        &format!("{},{} -- {:?}", tx, ty, tile.perceptual_hash()),
                        SCALE,
                        SCALE * 16.0,
                        SCALE * 16.0,
                        RED,
                    );
                }
            }
        }
    }
}
pub struct SelectedSprite {
    pub selected_sprite: Option<mappy::sprites::TrackID>,
    pub dims: (usize, usize),
}
impl Deco for SelectedSprite {
    #[allow(
        clippy::similar_names,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn draw(&mut self, mappy: &MappyState) {
        if is_mouse_button_down(MouseButton::Left) {
            // selected_sprite = None;
            for track in &mappy.live_tracks {
                let (mx, my) = mouse_position();
                if mappy::sprites::overlapping_sprite(
                    (mx / SCALE) as usize,
                    (my / SCALE) as usize,
                    2,
                    2,
                    &[*track.current_data()],
                ) {
                    self.selected_sprite = Some(track.id);
                }
            }
        }
        if let Some(track) = self
            .selected_sprite
            .and_then(|track_id| mappy.live_tracks.iter().find(|t| t.id == track_id))
        {
            let (wx, wy) = track.current_point();
            let (base_sx, base_sy) = mappy.world_to_screen(wx, wy);
            draw_rectangle_lines(
                base_sx as f32 * SCALE,
                base_sy as f32 * SCALE,
                8.0 * SCALE,
                f32::from(track.current_data().height()) * SCALE,
                1.0 * SCALE,
                BLUE,
            );
            let data = track.current_data();
            draw_text(
                &format!("{},{} -- {}", wx, wy, data.key()),
                self.dims.0 as f32 * SCALE - 100.0 * SCALE,
                SCALE * 16.0,
                SCALE * 16.0,
                BLUE,
            );
        }
    }
}
