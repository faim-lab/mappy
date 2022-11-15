use mappy::*;
use retro_rs::*;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub struct ScrollDumper {
    csv: std::fs::File,
    fm2_path: PathBuf,
    encoder: video_rs::Encoder,
    enc_time: video_rs::Time,
    fb: ndarray::Array3<u8>,
    // Since we don't output relative scrolls every frame,
    scroll: (i32, i32),
    frame_counter: usize,
    frame_time: video_rs::Time,
}

impl ScrollDumper {
    pub fn new(data_root: &Path, rom_name: &str) -> Self {
        video_rs::init();

        let date_str = format!("{}", chrono::Local::now().format("%Y-%m-%d-%H-%M-%S"));
        let data_folder = data_root
            .join(Path::new(&rom_name))
            .join(Path::new(&date_str));
        std::fs::create_dir_all(data_folder.clone()).unwrap();
        let csv_path = data_folder.join("scrolls.csv");
        let mut csv = std::fs::File::create(csv_path).unwrap();
        csv.write_all("x,y\n".as_bytes()).unwrap();
        let fm2_path = data_folder.join("inputs.fm2");
        let destination: video_rs::Locator = data_folder.join(Path::new("video.mp4")).into();
        // TODO remove constants
        let settings = video_rs::EncoderSettings::for_h264_yuv420p(256, 240, false);
        let encoder =
            video_rs::Encoder::new(&destination, settings).expect("failed to create video encoder");
        Self {
            csv,
            fm2_path,
            encoder,
            fb: ndarray::Array3::zeros((240, 256, 3)),
            enc_time: video_rs::Time::zero(),
            scroll: (0, 0),
            frame_time: std::time::Duration::from_nanos(1_000_000_000 / 60).into(),
            frame_counter: 0,
        }
    }
    pub fn update(&mut self, mappy: &MappyState, emu: &Emulator) {
        self.frame_counter += 1;
        //if self.frame_counter % self.output_interval == 0 {
        emu.copy_framebuffer_rgb888(self.fb.as_slice_mut().unwrap())
            .expect("Couldn't copy emulator framebuffer");
        self.encoder.encode(&self.fb, &self.enc_time).unwrap();
        self.enc_time = self.enc_time.aligned_with(&self.frame_time).add();
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
        //}
    }
    pub fn finish(mut self, inputs: &[[Buttons; 2]]) {
        self.encoder.finish().unwrap();
        self.csv.flush().unwrap();
        mappy::write_fm2(inputs, &self.fm2_path);
    }
}
