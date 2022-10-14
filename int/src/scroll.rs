use mappy::*;
use retro_rs::*;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub struct ScrollDumper {
    csv: std::fs::File,
    fm2_path: PathBuf,
    image_folder: PathBuf,
    // Since we don't output relative scrolls every frame,
    scroll: (i32, i32),
    output_interval: usize,
    frame_counter: usize,
}

impl ScrollDumper {
    pub fn new(data_root: &Path, rom_name: &str) -> Self {
        let date_str = format!("{}", chrono::Local::now().format("%Y-%m-%d-%H-%M-%S"));
        let image_folder = data_root
            .join(Path::new(&rom_name))
            .join(Path::new(&date_str));
        std::fs::create_dir_all(image_folder.clone()).unwrap();
        let base_path = data_root.join(Path::new(&rom_name));
        let csv_path = base_path.join(Path::new(&(date_str.clone() + ".csv")));
        let mut csv = std::fs::File::create(csv_path).unwrap();
        csv.write_all("x,y\n".as_bytes()).unwrap();
        let fm2_path = base_path.join(Path::new(&(date_str + ".fm2")));
        Self {
            csv,
            fm2_path,
            image_folder,
            scroll: (0, 0),
            output_interval: 7,
            frame_counter: 0,
        }
    }
    pub fn update(&mut self, mappy: &MappyState, fb: &mut [u8], emu: &Emulator) {
        self.frame_counter += 1;
        if self.frame_counter % self.output_interval == 0 {
            use image::ImageBuffer;
            emu.copy_framebuffer_rgba8888(fb)
                .expect("Couldn't copy emulator framebuffer");
            let (w, h) = emu.framebuffer_size();
            let img: ImageBuffer<image::Rgba<u8>, &mut [u8]> =
                ImageBuffer::from_raw(w as u32, h as u32, fb).unwrap();
            img.save(format!(
                "{}/{}.png",
                self.image_folder.display(),
                self.frame_counter
            ))
            .unwrap();
            // println!(
            //     "{},{},{}",
            //     self.frame_counter,
            //     mappy.scroll.0 - self.scroll.0,
            //     mappy.scroll.1 - self.scroll.1
            // );
            self.csv
                .write_fmt(format_args!(
                    "{},{}\n",
                    mappy.scroll.0 - self.scroll.0,
                    mappy.scroll.1 - self.scroll.1
                ))
                .expect("Couldn't write scroll data to csv");
            self.scroll = mappy.scroll;
        }
    }
    pub fn finish(self, inputs: &[[Buttons; 2]]) {
        mappy::write_fm2(inputs, &self.fm2_path);
    }
}
