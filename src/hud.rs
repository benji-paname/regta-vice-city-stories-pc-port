//! HUD textures and radar tiles are loaded from the user's disc. Layout is a
//! PC adaptation. Health/armor follow combat; economy is initial gameplay state.
//! the clock stays at noon while world visibility/time of day stay at noon.
use crate::render::{HEIGHT, WIDTH};
use crate::{
    iso::{Disc, Result},
    texture::Texture,
};

#[derive(Debug, serde::Serialize)]
pub struct PlayerStatus {
    pub health: u32,
    pub armor: u32,
    pub money: u32,
    pub hour: u32,
    pub minute: u32,
    pub wanted_level: u32,
    pub wanted_flashing: bool,
    pub mission_blip: Option<[f32; 3]>,
}
impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            health: 100,
            armor: 0,
            money: 0,
            hour: 12,
            minute: 0,
            wanted_level: 0,
            wanted_flashing: false,
            mission_blip: None,
        }
    }
}

pub struct Layer {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
impl Layer {
    fn label(&mut self, x: i32, y: i32, line: &str, color: [u8; 4]) {
        for (i, ch) in line.chars().enumerate() {
            for (row, bits) in glyph(ch).iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(
                            [x + i as i32 * 12 + col * 2, y + row as i32 * 2, 2, 2],
                            color,
                        );
                    }
                }
            }
        }
    }
    fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
            rgba: vec![0; width * height * 4],
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let p = &mut self.rgba[(y as usize * self.width + x as usize) * 4..][..4];
        let a = color[3] as u32;
        let back = p[3] as u32 * (255 - a) / 255;
        let total = a + back;
        if total == 0 {
            return;
        }
        for c in 0..3 {
            p[c] = ((color[c] as u32 * a + p[c] as u32 * back) / total) as u8;
        }
        p[3] = total as u8;
    }
    fn rect(&mut self, r: [i32; 4], color: [u8; 4]) {
        for y in r[1]..r[1] + r[3] {
            for x in r[0]..r[0] + r[2] {
                self.pixel(x, y, color);
            }
        }
    }
    fn sprite(&mut self, t: &Texture, dst: [i32; 4], source: [usize; 4], fraction: f32) {
        let width = (dst[2] as f32 * fraction.clamp(0., 1.)).round() as i32;
        for y in 0..dst[3] {
            for x in 0..width {
                let sx = source[0] + x as usize * source[2] / dst[2] as usize;
                let sy = source[1] + y as usize * source[3] / dst[3] as usize;
                let p = (sy * t.width + sx) * 4;
                self.pixel(dst[0] + x, dst[1] + y, t.rgba[p..p + 4].try_into().unwrap());
            }
        }
    }
    fn icon(&mut self, t: &Texture, dst: [i32; 4]) {
        self.sprite(t, dst, [0, 0, t.width, t.height], 1.);
    }
    pub fn composite(&self, pixels: &mut [u32]) {
        for y in 0..self.height.min(HEIGHT.saturating_sub(self.y)) {
            for x in 0..self.width.min(WIDTH.saturating_sub(self.x)) {
                let src = &self.rgba[(y * self.width + x) * 4..][..4];
                let a = src[3] as u32;
                if a == 0 {
                    continue;
                }
                let dst = &mut pixels[(self.y + y) * WIDTH + self.x + x];
                *dst = (0..3)
                    .map(|c| {
                        ((src[c] as u32 * a + ((*dst >> (16 - c * 8)) & 255) * (255 - a)) / 255)
                            << (16 - c * 8)
                    })
                    .sum();
            }
        }
    }
}

pub fn options(mode: crate::settings::Gameplay, message: &str) -> Layer {
    let mut panel = Layer::new(140, 105, 680, 330);
    panel.rect([0, 0, 680, 330], [10, 18, 25, 240]);
    panel.label(28, 24, "OPTIONS DE JEU", [240, 110, 170, 255]);
    for (y, value, label) in [
        (
            78,
            crate::settings::Gameplay::OriginalPs2,
            "1  ORIGINAL PS2",
        ),
        (122, crate::settings::Gameplay::Modern, "2  MODERNE"),
    ] {
        if mode == value {
            panel.rect([16, y - 10, 648, 36], [35, 93, 108, 255]);
        }
        panel.label(30, y, label, [255, 255, 255, 255]);
    }
    panel.label(
        28,
        174,
        if mode.modern() {
            "C : ACCROUPIR / SE RELEVER"
        } else {
            "COMMANDES D'ORIGINE"
        },
        [185, 225, 235, 255],
    );
    panel.label(
        28,
        198,
        if mode.modern() {
            "ESPACE FACE A UN MURET : FRANCHIR"
        } else {
            "AJOUTS DE DEPLACEMENT DESACTIVES"
        },
        [185, 225, 235, 255],
    );
    panel.label(28, 246, message, [245, 190, 115, 255]);
    panel.label(28, 290, "F3 / ECHAP : REPRENDRE", [255, 255, 255, 255]);
    panel
}

pub struct GameHud {
    numbers: Texture,
    fist: Texture,
    outline: Texture,
    bars: [Texture; 4],
    disc: Texture,
    back: Texture,
    centre: Texture,
    north: Texture,
    tiles: Vec<Texture>,
}
impl GameHud {
    pub fn load(disc: &mut Disc, game: &[u8]) -> Result<Self> {
        let get = |name| Texture::load_embedded_named(game, "hud", name);
        let mut tiles = Vec::with_capacity(64);
        for i in 0..64 {
            let t = Texture::load_streamed_named(disc, game, &format!("RADAR{i:02}"))?;
            if [t.width, t.height] != [128, 128] {
                return Err("Invalid native radar tile size".into());
            }
            tiles.push(t);
        }
        Ok(Self {
            numbers: get("hudnumbers")?,
            fist: get("fist")?,
            outline: get("bar_outline")?,
            bars: [
                get("bar_inside1dark")?,
                get("bar_inside2dark")?,
                get("bar_inside1")?,
                get("bar_inside2")?,
            ],
            disc: get("radardisc")?,
            back: get("radardiscback")?,
            centre: get("radar_centre")?,
            north: get("radar_north")?,
            tiles,
        })
    }
    fn numbers(&self, layer: &mut Layer, text: &str, x: i32, y: i32, green: bool) {
        for (i, ch) in text.chars().enumerate() {
            let cell = match ch {
                '0'..='9' => ch as usize - '0' as usize + if green { 11 } else { 0 },
                ':' => 10,
                '$' => 21,
                _ => continue,
            };
            layer.sprite(
                &self.numbers,
                [x + i as i32 * 17, y, 24, 24],
                [(cell % 8) * 16, (cell / 8) * 16, 16, 16],
                1.,
            );
        }
    }
    pub fn layers(
        &self,
        state: &PlayerStatus,
        position: [f32; 3],
        yaw: f32,
        heading: f32,
        exterior: bool,
    ) -> [Layer; 3] {
        let mut status = Layer::new(WIDTH - 264, 16, 248, 148);
        self.numbers(
            &mut status,
            &format!("{:02}:{:02}", state.hour % 24, state.minute % 60),
            60,
            0,
            false,
        );
        self.numbers(
            &mut status,
            &format!("${:08}", state.money.min(99_999_999)),
            60,
            89,
            true,
        );
        self.draw_bar(&mut status, 32, state.armor, 0, [20, 210, 220, 255]);
        self.draw_bar(&mut status, 59, state.health, 1, [220, 80, 150, 255]);
        self.icon_fist(&mut status);
        if state.wanted_level > 0 && !state.wanted_flashing {
            for i in 0..6 {
                let sx = 60 + i as i32 * 17;
                let sy = 120;
                let active = i < state.wanted_level as usize;
                let col = if active {
                    [255, 215, 0, 255]
                } else {
                    [45, 45, 45, 140]
                };
                draw_star(&mut status, sx, sy, col);
            }
        }
        let mut radar = Layer::new(20, HEIGHT - 180, 160, 160);
        radar.icon(&self.back, [0, 0, 160, 160]);
        let (s, c) = yaw.sin_cos();
        // PS2 0x114e68..0x114efc: 8x8, 500m tiles, X starts -2000,
        // Y starts +2000, half-texel inset (127*t+0.5)/128. Native
        // initial radar range is 256 (0x1109c8..0x1109e8).
        if exterior {
            for y in 8..152 {
                for x in 8..152 {
                    let dx = x as f32 + 0.5 - 80.;
                    let dy = 80. - y as f32 - 0.5;
                    if dx * dx + dy * dy > 71. * 71. {
                        continue;
                    }
                    let world = [
                        position[0] + (c * dx + s * dy) * 256. / 71.,
                        position[1] + (-s * dx + c * dy) * 256. / 71.,
                    ];
                    if let Some((tile, uv)) = radar_sample(world) {
                        let t = &self.tiles[tile];
                        let p = (uv[1] * t.width + uv[0]) * 4;
                        radar.pixel(x, y, t.rgba[p..p + 4].try_into().unwrap());
                    }
                }
            }
        }
        if let Some(blip) = state.mission_blip {
            let wx = blip[0] - position[0];
            let wy = blip[1] - position[1];
            let rx = (c * wx - s * wy) * 71. / 256.;
            let ry = (s * wx + c * wy) * 71. / 256.;
            let dist = (rx * rx + ry * ry).sqrt();
            let (bx, by) = if dist > 68.0 {
                (rx / dist * 68.0, ry / dist * 68.0)
            } else {
                (rx, ry)
            };
            let cx = (80.0 + bx) as i32;
            let cy = (80.0 - by) as i32;
            radar.rect([cx - 4, cy - 4, 8, 8], [0, 0, 0, 255]);
            radar.rect([cx - 3, cy - 3, 6, 6], [255, 230, 20, 255]);
        }
        radar.icon(&self.disc, [0, 0, 160, 160]);
        // Rotated original player marker, relative to the camera's radar view.
        let (ms, mc) = (heading - yaw).sin_cos();
        for y in -11..11 {
            for x in -11..11 {
                let sx = (mc * x as f32 + ms * y as f32 + 8.).floor() as i32;
                let sy = (-ms * x as f32 + mc * y as f32 + 8.).floor() as i32;
                if (0..16).contains(&sx) && (0..16).contains(&sy) {
                    let p = (sy as usize * 16 + sx as usize) * 4;
                    radar.pixel(
                        80 + x,
                        80 + y,
                        self.centre.rgba[p..p + 4].try_into().unwrap(),
                    );
                }
            }
        }
        radar.icon(
            &self.north,
            [
                (80. - s * 70.) as i32 - 8,
                (80. - c * 70.) as i32 - 8,
                16,
                16,
            ],
        );
        let mut message = Layer::new((WIDTH - 320) / 2, (HEIGHT - 80) / 2, 320, 80);
        if state.health == 0 {
            for (line, y, color) in [
                ("HORS COMBAT", 10, [225, 85, 145, 255]),
                ("R : RECOMMENCER", 48, [255, 255, 255, 255]),
            ] {
                let scale = if y == 10 { 3 } else { 2 };
                let width = line.len() * 6 * scale;
                let x = (320 - width) / 2;
                for (i, ch) in line.chars().enumerate() {
                    for (row, bits) in glyph(ch).iter().enumerate() {
                        for col in 0..5 {
                            if bits & (1 << (4 - col)) != 0 {
                                let r = [
                                    (x + i * 6 * scale + col * scale) as i32,
                                    (y + row * scale) as i32,
                                    scale as i32,
                                    scale as i32,
                                ];
                                message.rect([r[0] + 2, r[1] + 2, r[2], r[3]], [0, 0, 0, 255]);
                                message.rect(r, color);
                            }
                        }
                    }
                }
            }
        }
        [status, radar, message]
    }
    fn icon_fist(&self, layer: &mut Layer) {
        layer.icon(&self.fist, [170, 0, 78, 78]);
    }
    fn draw_bar(&self, layer: &mut Layer, y: i32, value: u32, kind: usize, color: [u8; 4]) {
        let r = [59, y, 106, 24];
        layer.sprite(&self.bars[kind], r, [16, 0, 48, 16], 1.);
        layer.sprite(
            &self.bars[kind + 2],
            r,
            [16, 0, 48, 16],
            value.min(100) as f32 / 100.,
        );
        layer.sprite(&self.outline, r, [16, 0, 48, 16], 1.);
        layer.rect([43, y + 2, 10, 20], [0, 0, 0, 255]);
        layer.rect([38, y + 7, 20, 10], [0, 0, 0, 255]);
        layer.rect([46, y + 4, 4, 16], color);
        layer.rect([41, y + 10, 14, 4], color);
    }
}

fn draw_star(layer: &mut Layer, x: i32, y: i32, color: [u8; 4]) {
    let pattern = [
        0b00100,
        0b11111,
        0b01110,
        0b10101,
        0b01010,
    ];
    for (row, bits) in pattern.iter().enumerate() {
        for col in 0..5 {
            if (bits & (1 << (4 - col))) != 0 {
                layer.rect([x + col * 2, y + row as i32 * 2, 2, 2], color);
            }
        }
    }
}
fn radar_sample(world: [f32; 2]) -> Option<(usize, [usize; 2])> {
    if world.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let x = (world[0] + 2000.) / 500.;
    let y = (2000. - world[1]) / 500.;
    if !(0.0..8.0).contains(&x) || !(0.0..8.0).contains(&y) {
        return None;
    }
    Some((
        y.floor() as usize * 8 + x.floor() as usize,
        [
            (x.fract() * 127. + 0.5) as usize,
            (y.fract() * 127. + 0.5) as usize,
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radar_coordinates_match_native_tile_corners_and_edges() {
        assert_eq!(radar_sample([-2000., 2000.]), Some((0, [0, 0])));
        assert_eq!(radar_sample([-1500., 1500.]), Some((9, [0, 0])));
        assert_eq!(radar_sample([0., 0.]), Some((36, [0, 0])));
        assert_eq!(radar_sample([1999.99, -1999.99]), Some((63, [127, 127])));
        for p in [
            [2000., 0.],
            [0., -2000.],
            [-2000.1, 0.],
            [0., 2000.1],
            [f32::NAN, 0.],
        ] {
            assert!(radar_sample(p).is_none());
        }
    }
    #[test]
    fn transparent_hud_layers_preserve_scene_and_clip_at_edges() {
        let mut layer = Layer::new(WIDTH - 2, HEIGHT - 2, 4, 4);
        layer.rect([1, 1, 4, 4], [240, 20, 40, 128]);
        let mut pixels = vec![0x102030; WIDTH * HEIGHT];
        layer.composite(&mut pixels);
        assert_eq!(pixels[0], 0x102030);
        assert_eq!(pixels[(HEIGHT - 2) * WIDTH + WIDTH - 2], 0x102030);
        assert_ne!(pixels[WIDTH * HEIGHT - 1], 0x102030);
        assert_eq!(pixels.iter().filter(|&&p| p != 0x102030).count(), 1);
    }
    #[test]
    #[ignore = "Requires the user's local ISO"]
    fn native_hud_textures_decode() {
        use crate::{iso::Disc, texture::Texture};
        let mut disc = Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let names = [
            "hudnumbers",
            "bar_outline",
            "bar_inside1",
            "bar_inside2",
            "bar_inside1dark",
            "bar_inside2dark",
            "fist",
            "radardisc",
            "radardiscback",
            "radar_centre",
            "radar_north",
        ];
        let mut frame = crate::render::Frame::new();
        frame.pixels.fill(0x507080);
        for (i, name) in names.iter().enumerate() {
            let t = Texture::load_embedded_named(&game, "hud", name).unwrap();
            println!("{name}: {}x{} alpha={}", t.width, t.height, t.has_alpha);
            let ox = (i % 4) * 235;
            let oy = (i / 4) * 170;
            text(&mut frame.pixels, ox, oy, name, 0xffffff);
            for y in 0..t.height.min(145) {
                for x in 0..t.width.min(230) {
                    let p = &t.rgba[(y * t.width + x) * 4..][..4];
                    let dst = &mut frame.pixels[(oy + 20 + y) * WIDTH + ox + x];
                    let a = p[3] as u32;
                    let rgb = (0..3)
                        .map(|c| {
                            ((p[c] as u32 * a + ((*dst >> (16 - c * 8)) & 255) * (255 - a)) / 255)
                                << (16 - c * 8)
                        })
                        .sum::<u32>();
                    *dst = rgb;
                }
            }
        }
        frame
            .write_bmp(std::path::Path::new("../../work/hud-native-atlas.bmp"))
            .unwrap();
        for i in 0..64 {
            let t =
                Texture::load_streamed_named(&mut disc, &game, &format!("RADAR{i:02}")).unwrap();
            assert_eq!([t.width, t.height], [128, 128]);
        }
        let hud = GameHud::load(&mut disc, &game).unwrap();
        let layers = hud.layers(&PlayerStatus::default(), [239., -676., 9.], 0., 0., true);
        let interior = hud.layers(&PlayerStatus::default(), [239., -676., 9.], 0., 0., false);
        assert_eq!(layers[0].rgba, interior[0].rgba);
        assert_ne!(layers[1].rgba, interior[1].rgba);
        assert_eq!(
            layers[1].rgba[3], 0,
            "radar corners must remain transparent"
        );
    }
}

pub fn guide(pixels: &mut [u32], zone: &str, walking: bool, driving: bool, show_help: bool) {
    if !show_help {
        return;
    }
    let mode = if driving {
        "CONDUITE"
    } else if walking {
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
        panel(pixels, 10, HEIGHT - 87, WIDTH - 20, 77);
        text(
            pixels,
            20,
            HEIGHT - 77,
            "F3: ORIGINAL PS2 / MODERNE | C: ACCROUPIR (MODERNE)",
            0xbceaff,
        );
        text(
            pixels,
            20,
            HEIGHT - 57,
            if driving {
                "Z/W: ACCELERER | S: FREIN / RECUL | ESPACE: FREIN A MAIN"
            } else {
                "ZQSD / WASD: DEPLACER | FLECHES: VUE | MAJ: ACCELERER"
            },
            0xffffff,
        );
        text(
            pixels,
            20,
            HEIGHT - 42,
            if driving {
                "Q/A-D: CONDUIRE | H: GIROPHARE | 1-4: LIEU | T: TEXTURES"
            } else if walking {
                "ESPACE: SAUT | TAB: ARME | CLIC G/F: TIR | CLIC D/V: VISER"
            } else {
                "ESPACE / CTRL: HAUTEUR | 1-4: LIEU | T: TEXTURES"
            },
            0xffffff,
        );
        text(
            pixels,
            20,
            HEIGHT - 27,
            if driving {
                "ARRET + E: SORTIR | R: REINITIALISER | ECHAP: QUITTER"
            } else {
                "F5: SAUVER | F9: CHARGER | G: MARCHE | E: VEHICULE | F1: AIDE"
            },
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
