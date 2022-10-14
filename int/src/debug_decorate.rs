use super::*;
use macroquad::prelude::*;
use mappy::*;
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
                        (x as f32 * SCALE) as f32,
                        (y as f32 * SCALE) as f32,
                        TILE_SIZE as f32 * SCALE,
                        TILE_SIZE as f32 * SCALE,
                        Color::new(
                            (idx * 127 % 256) as f32 / 255.,
                            (idx * 33 % 256) as f32 / 255.,
                            (idx * 61 % 256) as f32 / 255.,
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
    fn draw(&mut self, mappy: &MappyState) {
        for track in mappy.live_tracks.iter() {
            let col = Color::new(
                (*(track.positions[0].0) * 31 % 256) as f32 / 255.,
                (*(track.positions[0].0) * 127 % 256) as f32 / 255.,
                (*(track.positions[0].0) * 91 % 256) as f32 / 255.,
                1.,
            );
            let startp = Vec2::new(
                ((track.positions[0].1).0 + track.positions[0].2.x as i32 - mappy.scroll.0) as f32,
                ((track.positions[0].1).1 + track.positions[0].2.y as i32 - mappy.scroll.1) as f32,
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
                    let x0 = sx0 + (sd0.x as i32) - mappy.scroll.0;
                    let y0 = sy0 + (sd0.y as i32) - mappy.scroll.1;
                    let mappy::sprites::At(_, (sx1, sy1), sd1) = pair[1];
                    let x1 = sx1 + (sd1.x as i32) - mappy.scroll.0;
                    let y1 = sy1 + (sd1.y as i32) - mappy.scroll.1;
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
                    sd.x as f32 * SCALE,
                    sd.y as f32 * SCALE,
                    sd.width() as f32 * SCALE,
                    sd.height() as f32 * SCALE,
                    2.0,
                    col,
                );
            }
        }
    }
}

pub struct LiveBlobs {}
impl Deco for LiveBlobs {
    fn draw(&mut self, mappy: &MappyState) {
        for blob in mappy.live_blobs.iter() {
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
    fn draw(&mut self, mappy: &MappyState) {
        for track in mappy.live_tracks.iter() {
            if track.get_is_avatar() {
                let mappy::sprites::At(_, (sx0, sy0), sd0) = track.positions.last().unwrap();
                let x0 = sx0 + (sd0.x as i32) - mappy.scroll.0;
                let y0 = sy0 + (sd0.y as i32) - mappy.scroll.1;
                draw_circle(x0 as f32 * SCALE, y0 as f32 * SCALE, 4.0, DARKBLUE);
            }
        }
    }
}
