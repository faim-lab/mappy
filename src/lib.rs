#![allow(clippy::many_single_char_names)]
mod framebuffer;
mod mappy;
mod pixels;
mod scrolling;
mod sprites;
mod tile;
pub use mappy::*;
use std::fs::File;
use std::path::Path;
use retro_rs::Buttons;

fn to_bitstring(b:Buttons) -> String {
    format!("{}{}{}{}{}{}{}{}",
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

pub fn write_fm2(inputs:&[[Buttons;2]], path:&Path) {
    use uuid::Uuid;
    use std::io::Write;
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

pub fn from_bitstring(bs:[bool;8]) -> Buttons {
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

pub fn read_fm2(inputs:&mut Vec<[Buttons;2]>, path:&Path) {
    use std::io::{BufReader, BufRead};
    let file = File::open(path).expect("Couldn't open file");
    let reader = BufReader::new(file);
    for line in reader.lines() {
        if let Ok(line) = line {
            // scan ahead to second |
            let mut pipenum = 0;
            let mut bitstr = [false;8];
            let mut bitstr_idx = 0;
            let mut buttons1:Option<Buttons> = None;
            let mut buttons2:Option<Buttons> = None;
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
                _ => ()
            }
        }
    }
}
