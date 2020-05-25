use retro_rs::Emulator;

pub struct Framebuffer {
    pub fb: Vec<u8>,
    pub w: usize,
    pub h: usize,
}
impl Framebuffer {
    pub fn new(w: usize, h: usize) -> Self {
        Framebuffer {
            fb: vec![0; w * h],
            w,
            h,
        }
    }
    pub fn read_from(&mut self, emu: &Emulator) {
        // TODO: make fb.fb work on u64s for 8 pixel spans?  measure!
        emu.copy_framebuffer_rgb332(&mut self.fb).expect("Couldn't get FB");
    }
}
