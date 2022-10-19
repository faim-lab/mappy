use super::*;
const SPEEDS: [usize; 10] = [0, 1, 5, 15, 30, 60, 120, 240, 300, 360];
pub struct Playback {
    pub start: std::time::Instant,
    pub frame: u64,
    pub inputs: Vec<[Buttons; 2]>,
    pub replay_inputs: Vec<[Buttons; 2]>,
    pub replay_index: usize,
    pub speed: usize,
    pub accum: f32,
}
impl Playback {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            frame: 0,
            inputs: Vec::with_capacity(10_000),
            replay_inputs: vec![],
            replay_index: 0,
            speed: 5,
            accum: 0.0,
        }
    }
    pub fn update_speed(&mut self) {
        if is_key_pressed(KeyCode::O) {
            self.speed = if self.speed == 0
                || is_key_down(KeyCode::LeftShift)
                || is_key_down(KeyCode::RightShift)
            {
                0
            } else {
                self.speed - 1
            };
            println!("Speed {:?}", self.speed);
        } else if is_key_pressed(KeyCode::P) {
            self.speed = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                6
            } else {
                (self.speed + 1).min(SPEEDS.len() - 1)
            };
            println!("Speed {:?}", self.speed);
        }
    }
    pub fn replay(&mut self, path: &Path) {
        self.frame = 0;
        self.inputs.clear();
        self.replay_inputs.clear();
        mappy::read_fm2(&mut self.replay_inputs, path);
        self.replay_index = 0;
    }
    pub fn step(&mut self, dt: f32, mut f: impl FnMut(f32, [Buttons; 2])) {
        // Add dt (s) * multiplier (frame/s) to get number of frames.
        // e.g. 60 * 1/60 = 1
        self.accum += SPEEDS[self.speed] as f32 * dt;
        while self.accum >= 1.0 {
            let buttons = if self.replay_index >= self.replay_inputs.len() {
                Buttons::new()
                    .up(is_key_down(KeyCode::W))
                    .down(is_key_down(KeyCode::S))
                    .left(is_key_down(KeyCode::A))
                    .right(is_key_down(KeyCode::D))
                    .select(is_key_down(KeyCode::G))
                    .start(is_key_down(KeyCode::H))
                    .b(is_key_down(KeyCode::J))
                    .a(is_key_down(KeyCode::K))
            } else {
                self.replay_index += 1;
                self.replay_inputs[self.replay_index - 1][0]
            };
            self.inputs.push([buttons, Buttons::new()]);

            f(self.accum, *self.inputs.last().unwrap());
            self.frame += 1;
            if self.frame % 300 == 0 {
                // println!("Scroll: {:?} : {:?}", mappy.splits, mappy.scroll);
                // println!("Known tiles: {:?}", mappy.tiles.gfx_count());
                println!(
                    "Net: {:} for {:} inputs, avg {:}",
                    self.start.elapsed().as_secs_f64(),
                    self.frame,
                    self.start.elapsed().as_secs_f64() / (self.frame as f64)
                );
            }
            self.accum -= 1.0;
        }
    }
}
