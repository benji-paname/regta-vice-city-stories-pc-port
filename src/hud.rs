//! Small original bitmap lettering for the prototype's keyboard guide.
use crate::render::{HEIGHT, WIDTH};

pub fn guide(pixels: &mut [u32], zone: &str, walking: bool, show_help: bool) {
    let mode = if walking {
        "MARCHE EXPERIMENTALE"
    } else {
        "CAMERA LIBRE"
    };
    panel(pixels, 10, 10, 530, 30);
    text(
        pixels,
        20,
        19,
        &format!("VCS RUST | {zone} | {mode}"),
        0xffffff,
    );
    if show_help {
        panel(pixels, 10, HEIGHT - 67, WIDTH - 20, 57);
        text(
            pixels,
            20,
            HEIGHT - 57,
            "ZQSD / WASD: DEPLACER | FLECHES: VUE | MAJ: ACCELERER",
            0xffffff,
        );
        text(
            pixels,
            20,
            HEIGHT - 42,
            if walking {
                "ESPACE: SAUTER | 1-4: LIEU | F2: VUE | T: TEXTURES"
            } else {
                "ESPACE / CTRL: HAUTEUR | 1-4: LIEU | T: TEXTURES"
            },
            0xffffff,
        );
        text(
            pixels,
            20,
            HEIGHT - 27,
            "G: MARCHER | R: REINITIALISER | F1: AIDE | ECHAP: QUITTER",
            0xbceaff,
        );
    }
}
fn panel(pixels: &mut [u32], x: usize, y: usize, w: usize, h: usize) {
    for row in y..(y + h).min(HEIGHT) {
        for col in x..(x + w).min(WIDTH) {
            let p = &mut pixels[row * WIDTH + col];
            *p = ((*p >> 2) & 0x3f3f3f) + 0x050b10;
        }
    }
}
fn text(pixels: &mut [u32], x: usize, y: usize, line: &str, color: u32) {
    for (letter, ch) in line.chars().enumerate() {
        let bits = glyph(ch);
        for (row, bits) in bits.iter().enumerate() {
            for col in 0..5 {
                let x = x + letter * 12 + col * 2;
                let y = y + row * 2;
                if bits & (1 << (4 - col)) != 0 && x + 1 < WIDTH && y + 1 < HEIGHT {
                    pixels[y * WIDTH + x] = color;
                    pixels[y * WIDTH + x + 1] = color;
                    pixels[(y + 1) * WIDTH + x] = color;
                    pixels[(y + 1) * WIDTH + x + 1] = color;
                }
            }
        }
    }
}
pub fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 27, 17],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        _ => [0; 7],
    }
}
