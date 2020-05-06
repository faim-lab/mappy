
pub fn rgb332_to_rgb888(col:u8) -> (u8,u8,u8) {
    let col = col as u32;
    let r = (((col & 0b1110_0000) >> 5) * 255) / 8;
    let g = (((col & 0b0001_1100) >> 2) * 255) / 8;
    let b = ((col & 0b0000_0011) * 255) / 4;
    assert!(r <= 255);
    assert!(g <= 255);
    assert!(b <= 255);
    (r as u8,g as u8,b as u8)
}

pub fn rgb888_to_rgb332(r:u8, g:u8, b:u8) -> u8 {
    let r = ((r as u32 * 8) / 256) as u8;
    let g = ((g as u32 * 8) / 256) as u8;
    let b = ((b as u32 * 4) / 256) as u8;
    assert!(r <= 7);
    assert!(g <= 7);
    assert!(b <= 3);
    (r << 5) + (g << 2) + b
}
