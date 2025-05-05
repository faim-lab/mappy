#![allow(clippy::many_single_char_names)]
mod framebuffer;
mod mappy;
pub mod metaroom;
mod ringbuffer;
pub mod room;
mod screen;
pub mod sprites;
pub mod tile;
pub mod time;
pub use crate::mappy::*;

use retro_rs::Buttons;
use std::fs::File;
use std::path::Path;
pub use tile::TILE_SIZE;

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Time(usize);

impl std::ops::Add<Time> for Time {
    type Output = Time;
    fn add(self, other: Time) -> Self::Output {
        Time(self.0 + other.0)
    }
}
impl std::ops::Sub<Time> for Time {
    type Output = Time;
    fn sub(self, other: Time) -> Self::Output {
        Time(self.0 - other.0)
    }
}
impl std::ops::Add<usize> for Time {
    type Output = Time;
    fn add(self, other: usize) -> Self::Output {
        Time(self.0 + other)
    }
}
impl std::ops::Sub<usize> for Time {
    type Output = Time;
    fn sub(self, other: usize) -> Self::Output {
        Time(self.0 - other)
    }
}

impl std::ops::Deref for Time {
    type Target = usize;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}
impl Rect {
    #[must_use]
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }
    #[inline]
    #[must_use]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.x <= x && x < self.x + self.w as i32 && self.y <= y && y < self.y + self.h as i32
    }
    #[must_use]
    pub fn contains_rect(&self, r: &Rect) -> bool {
        self.union(r) == *self
    }
    #[must_use]
    pub fn overlaps(&self, r: &Rect) -> bool {
        self.x < (r.x + r.w as i32)
            && r.x < (self.x + self.w as i32)
            && self.y < (r.y + r.h as i32)
            && r.y < (self.y + self.h as i32)
    }
    #[must_use]
    pub fn expand(&self, amt: u32) -> Rect {
        Rect {
            x: self.x - amt as i32,
            y: self.y - amt as i32,
            w: self.w + amt * 2,
            h: self.h + amt * 2,
        }
    }
    #[must_use]
    pub fn union(&self, other: &Rect) -> Rect {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w as i32).max(other.x + other.w as i32);
        let y1 = (self.y + self.h as i32).max(other.y + other.h as i32);
        Rect {
            x: x0,
            y: y0,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        }
    }
    #[must_use]
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let right = (self.x + self.w as i32).min(other.x + other.w as i32);
        let top = self.y.max(other.y);
        let bot = (self.y + self.h as i32).min(other.y + other.h as i32);
        if right - left > 0 && bot - top > 0 {
            Some(Rect {
                x: left,
                y: top,
                w: (right - left) as u32,
                h: (bot - top) as u32,
            })
        } else {
            None
        }
    }
    #[must_use]
    pub fn area(&self) -> u32 {
        self.w * self.h
    }
}

#[must_use]
fn to_bitstring(b: Buttons) -> String {
    format!(
        "{}{}{}{}{}{}{}{}",
        if b.get_right() { "R" } else { "." },
        if b.get_left() { "L" } else { "." },
        if b.get_down() { "D" } else { "." },
        if b.get_up() { "U" } else { "." },
        if b.get_start() { "T" } else { "." },
        if b.get_select() { "S" } else { "." },
        if b.get_b() { "B" } else { "." },
        if b.get_a() { "A" } else { "." }
    )
}

pub fn write_fm2(inputs: &[[Buttons; 2]], path: &Path) {
    use std::io::Write;
    use uuid::Uuid;
    let mut file = File::create(path).expect("Couldn't dump file");
    writeln!(file, "version 3").unwrap();
    writeln!(file, "palFlag 0").unwrap();
    writeln!(file, "NewPPU 1").unwrap();
    writeln!(file, "FDS 0").unwrap();
    writeln!(file, "fourscore 0").unwrap();
    writeln!(file, "port0 1").unwrap();
    writeln!(file, "port1 1").unwrap();
    writeln!(file, "binary 0").unwrap();
    writeln!(file, "length {}", inputs.len()).unwrap();
    writeln!(file, "romFilename Super Mario Bros.").unwrap();
    let guid = Uuid::new_v4();
    writeln!(file, "guid {}", guid).unwrap();
    // TODO fixme
    writeln!(file, "romChecksum 0").unwrap();
    for &[b1, b2] in inputs.iter() {
        writeln!(file, "||{}|{}|", to_bitstring(b1), to_bitstring(b2)).unwrap();
    }
}

pub fn from_bitstring(bs: [bool; 8]) -> Buttons {
    Buttons::new()
        .right(bs[0])
        .left(bs[1])
        .down(bs[2])
        .up(bs[3])
        .start(bs[4])
        .select(bs[5])
        .b(bs[6])
        .a(bs[7])
}

pub fn read_fm2(inputs: &mut Vec<[Buttons; 2]>, path: &Path) {
    use std::io::{BufRead, BufReader};
    let file = File::open(path).expect("Couldn't open file");
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        // scan ahead to second |
        let mut pipenum = 0;
        let mut bitstr = [false; 8];
        let mut bitstr_idx = 0;
        let mut buttons1: Option<Buttons> = None;
        let mut buttons2: Option<Buttons> = None;
        for c in line.chars() {
            if c == '|' {
                pipenum += 1;
                if pipenum == 3 {
                    buttons1 = Some(from_bitstring(bitstr));
                    bitstr_idx = 0;
                } else if pipenum == 4 {
                    buttons2 = Some(from_bitstring(bitstr));
                    break;
                }
            } else if pipenum >= 2 {
                assert!(bitstr_idx < 8);
                bitstr[bitstr_idx] = !(c == '.' || c == ' ');
                bitstr_idx += 1;
            }
        }
        match (buttons1, buttons2) {
            (Some(buttons1), Some(buttons2)) => {
                inputs.push([buttons1, buttons2]);
            }
            (Some(buttons1), None) => {
                inputs.push([buttons1, Buttons::new()]);
            }
            _ => (),
        }
    }
}
