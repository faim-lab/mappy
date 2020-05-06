use retro_rs::Emulator;
use crate::pixels;

pub struct Framebuffer{
    pub fb:Vec<u8>,
    pub w:usize,
    pub h:usize
}
impl Framebuffer {
    pub fn new(w:usize,h:usize) -> Self {
        Framebuffer {
            fb:vec![0;w*h],
            w,
            h
        }
    }
    pub fn read_from(&mut self, emu:&Emulator) {
        // TODO: make fb.fb work on u64s for 8 pixel spans?  measure!
        emu.for_each_pixel(|x,y,r,g,b| {
            let col = pixels::rgb888_to_rgb332(r,g,b);
            self.fb[y*self.w+x] = col;
        }).expect("Couldn't get FB");
    }
}
