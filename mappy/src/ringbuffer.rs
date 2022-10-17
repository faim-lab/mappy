// recording values of type buttons, button presses. Look at button struct in retro.rs
// in the ringvector, the head jumps to the beginning. There's never a total reset.
// get jump to definition
// usize starts at zero and goes up until how long the ring buffer is and then wraps around
// now is where the next write is going to happen

// in get, 0 could give you the last thing that was written. 1 could give you the thing before that

#[derive(Debug, Clone)]
pub struct RingBuffer<T: Copy> {
    buf: Vec<T>,
    now: usize,
}
impl<T: Copy> RingBuffer<T> {
    pub fn new(t: T, sz: usize) -> Self {
        RingBuffer {
            buf: vec![t; sz],
            now: 0,
        }
    }
    pub fn push(&mut self, t: T) {
        self.now = (self.now + 1) % self.buf.len();
        self.buf[self.now] = t;
    }
    pub fn get(&self, since: usize) -> T {
        // self.buf[(self.buf.len() - since) - 1]
        let mut idx = self.now as i64 - since as i64;
        while idx < 0 {
            idx += self.buf.len() as i64
        }
        self.buf[idx as usize]
    }
    pub fn get_sz(&self) -> usize {
        self.buf.len()
    }
}
impl<T: Copy + std::fmt::Debug> std::fmt::Display for RingBuffer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {:?}", self.buf, self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_get() {
        let mut rb = RingBuffer::new(0, 3);
        assert_eq!(rb.get(0), 0);
        assert_eq!(rb.get(1), 0);
        assert_eq!(rb.get(2), 0);
        assert_eq!(rb.get_sz(), 3);
        rb.push(1);
        assert_eq!(rb.get(0), 1);
        assert_eq!(rb.get(1), 0);
        assert_eq!(rb.get(2), 0);
        assert_eq!(rb.get_sz(), 3);
        rb.push(2);
        assert_eq!(rb.get(0), 2);
        assert_eq!(rb.get(1), 1);
        assert_eq!(rb.get(2), 0);
        assert_eq!(rb.get_sz(), 3);
        rb.push(3);
        assert_eq!(rb.get(0), 3);
        assert_eq!(rb.get(1), 2);
        assert_eq!(rb.get(2), 1);
        assert_eq!(rb.get_sz(), 3);
        rb.push(4);
        assert_eq!(rb.get(0), 4);
        assert_eq!(rb.get(1), 3);
        assert_eq!(rb.get(2), 2);
        assert_eq!(rb.get_sz(), 3);
        assert_eq!(rb.get(3), 4);
    }
}
