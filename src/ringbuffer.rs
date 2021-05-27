// recording values of type buttons, button presses. Look at button struct in retro.rs
// in the ringvector, the head jumps to the beginning. There's never a total reset.
// get jump to definition
// usize starts at zero and goes up until how long the ring buffer is and then wraps around
// now is where the next write is going to happen

// in get, 0 could give you the last thing that was written. 1 could give you the thing before that


#[derive(Debug, Clone)]
pub struct RingBuffer<T:Copy + std::fmt::Debug> {
    buf:Vec<T>,
    now:usize
}
impl<T:Copy + std::fmt::Debug> RingBuffer<T> {
    pub fn new(t:T, sz:usize) -> Self {
        RingBuffer{
            buf: vec![t;sz],
            now: 0,
        }
    }
    pub fn to_string(&self) -> String {
        format!("{:?} {:?}", self.buf, self.now)
    }
    pub fn push(&mut self, t:T) {
        self.now = (self.now + 1) % self.buf.len();
        self.buf[self.now] = t;
    }
    pub fn get(&self, since:usize) -> T {
        // self.buf[(self.buf.len() - since) - 1]
        let mut idx = self.now as i64 - since as i64;
        while idx < 0 { idx += self.buf.len() as i64 }
        self.buf[idx as usize]
    }
    pub fn get_sz(&self) -> usize {
        self.buf.len()
    }
}
