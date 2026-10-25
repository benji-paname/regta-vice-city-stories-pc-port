//! Native decoding of the compact PS2 RslRaster used by VCS world resources.
//! The resource table owns the material id; these records have no texture name.
//! Offsets are checked before reading. Rows remain in console texture order,
//! so UVs from the VIF stream can be sampled directly without an OpenGL flip.

use crate::iso::{bytes, u16le, u32le, Result};

#[derive(Clone, Debug)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    /// Top row first, RGBA8. Console alpha 0..128 is expanded to 0..255.
    pub rgba: Vec<u8>,
    pub depth: u8,
    pub has_alpha: bool,
    mips: Vec<Mip>,
}

#[derive(Clone, Debug)]
struct Mip {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Texture {
    /// A named streamed TXD uses the same raster layout as the embedded HUD.
    pub fn load_streamed_named(
        disc: &mut crate::iso::Disc,
        game: &[u8],
        name: &str,
    ) -> Result<Self> {
        let pool = u32le(game, 0x60)? as usize;
        let items = u32le(game, pool)? as usize;
        let count = u32le(game, pool + 8)? as usize;
        if count > 65536 {
            return Err("Invalid TXD pool".into());
        }
        bytes(game, items, count * 28)?;
        for slot in 0..count {
            let raw = bytes(game, items + slot * 28 + 8, 20)?;
            let end = raw.iter().position(|&c| c == 0).unwrap_or(20);
            if !std::str::from_utf8(&raw[..end])?.eq_ignore_ascii_case(name) {
                continue;
            }
            let manager = u32le(game, 0x7c)? as usize;
            let base = u32le(game, manager + 8)? as usize;
            let table = u32le(game, manager + 0xc8)? as usize;
            let row = table + (base + slot) * 24;
            let sector = u32le(game, row + 16)? as usize;
            let size = u32le(game, row + 20)? as usize;
            if size == 0 || size > 4096 {
                return Err("Invalid HUD streaming extent".into());
            }
            let data = disc.range("GTA3PS2.IMG", sector * 2048, size * 2048)?;
            if bytes(&data, 0, 4)? != b"xet\0" {
                return Err("Invalid HUD TXD magic".into());
            }
            let data = bytes(&data, 0, u32le(&data, 8)? as usize)?;
            let head = 0x28;
            let link = u32le(data, head)? as usize;
            if link < 8 || link == head || u32le(data, link)? as usize != head {
                return Err("Radar TXD must contain exactly one texture".into());
            }
            let raster = u32le(data, link - 8)? as usize;
            bytes(data, raster, 16)?;
            let format = u16le(data, raster + 6)?;
            let mut canonical = data.to_vec();
            canonical[raster..raster + 4].copy_from_slice(&0xccccccccu32.to_le_bytes());
            canonical[raster + 6..raster + 8].copy_from_slice(&(format | 0xc000).to_le_bytes());
            return decode_raster(&canonical, raster, 0);
        }
        Err(format!("Missing streamed TXD {name}").into())
    }

    /// Named textures in the embedded dictionaries use linked RslTexture rows.
    pub fn load_embedded_named(
        game: &[u8],
        dictionary_name: &str,
        texture_name: &str,
    ) -> Result<Self> {
        fn name(data: &[u8], at: usize, len: usize) -> Result<&str> {
            let raw = bytes(data, at, len)?;
            Ok(std::str::from_utf8(
                &raw[..raw.iter().position(|&c| c == 0).unwrap_or(len)],
            )?)
        }
        let pool = u32le(game, 0x60)? as usize;
        let items = u32le(game, pool)? as usize;
        let count = u32le(game, pool + 8)? as usize;
        if count > 65536 {
            return Err("Invalid texture dictionary pool".into());
        }
        bytes(game, items, count * 28)?;
        for i in 0..count {
            let row = items + i * 28;
            if !name(game, row + 8, 20)?.eq_ignore_ascii_case(dictionary_name) {
                continue;
            }
            let dictionary = u32le(game, row)? as usize;
            if dictionary < 32 {
                return Err("Texture dictionary is not embedded".into());
            }
            let head = dictionary + 8;
            let mut link = u32le(game, head)? as usize;
            let mut seen = std::collections::HashSet::new();
            while link != head {
                if link < 8 || seen.len() >= 1024 || !seen.insert(link) {
                    return Err("Invalid texture dictionary links".into());
                }
                let texture = link - 8;
                if name(game, texture + 16, 32)?.eq_ignore_ascii_case(texture_name) {
                    let raster = u32le(game, texture)? as usize;
                    if raster < 32 {
                        return Err("Missing embedded texture raster".into());
                    }
                    bytes(game, raster, 16)?;
                    let format = u16le(game, raster + 6)?;
                    let mut canonical = game.to_vec();
                    canonical[raster..raster + 4].copy_from_slice(&0xccccccccu32.to_le_bytes());
                    canonical[raster + 6..raster + 8]
                        .copy_from_slice(&(format | 0xc000).to_le_bytes());
                    return decode_raster(&canonical, raster, 0);
                }
                link = u32le(game, link)? as usize;
            }
        }
        Err(format!("Missing embedded texture {dictionary_name}/{texture_name}").into())
    }

    pub fn tinted(&self, tint: [u8; 4]) -> Self {
        let mut texture = self.clone();
        let apply = |pixels: &mut [u8]| {
            for pixel in pixels.chunks_exact_mut(4) {
                for c in 0..4 {
                    pixel[c] = ((pixel[c] as u16 * tint[c] as u16 + 127) / 255) as u8;
                }
            }
        };
        apply(&mut texture.rgba);
        for mip in &mut texture.mips {
            apply(&mut mip.rgba);
        }
        texture.has_alpha = texture.rgba.chunks_exact(4).any(|p| p[3] < 255);
        texture
    }
    /// Apply palette paint only to neutral body texels. Keep dark trim and
    /// chromatic atlas details (such as tail lamps) in their original colors.
    pub fn painted_vehicle_surface(&self, color: [u8; 3]) -> Self {
        let mut rgba = self.rgba.clone();
        for pixel in rgba.chunks_exact_mut(4) {
            let luminance =
                (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32)
                    / 255.0;
            let darkest = pixel[0].min(pixel[1]).min(pixel[2]) as f32;
            let brightest = pixel[0].max(pixel[1]).max(pixel[2]) as f32;
            let saturation = (brightest - darkest) / brightest.max(1.0);
            if luminance < 0.16 || saturation > 0.16 {
                continue;
            }
            // Retain contrast from the dark grayscale atlas while lifting
            // its midtones into the selected paint color. The previous
            // near-white mapping erased the exterior's panel and shadow detail.
            let detail = 0.55 + 0.45 * luminance.sqrt();
            for channel in 0..3 {
                pixel[channel] = (color[channel] as f32 * 1.12 * detail)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
        let mips = make_mips(self.width, self.height, &rgba);
        Self {
            width: self.width,
            height: self.height,
            rgba,
            depth: self.depth,
            has_alpha: self.has_alpha,
            mips,
        }
    }

    /// Nearest sampling with repeat addressing, including negative UVs.
    #[cfg(test)]
    pub fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return [1.0; 4];
        }
        let x = ((uv[0].rem_euclid(1.0) * self.width as f32) as usize).min(self.width - 1);
        let y = ((uv[1].rem_euclid(1.0) * self.height as f32) as usize).min(self.height - 1);
        let i = (y * self.width + x) * 4;
        let mut color = [0.0; 4];
        for c in 0..4 {
            color[c] = self.rgba[i + c] as f32 / 255.0;
        }
        color
    }

    pub fn mip_levels(&self) -> usize {
        self.mips.len() + 1
    }

    /// Borrow one complete RGBA mip for native graphics uploads.
    pub fn mip(&self, level: usize) -> Option<(usize, usize, &[u8])> {
        if level == 0 {
            Some((self.width, self.height, &self.rgba))
        } else {
            self.mips
                .get(level - 1)
                .map(|mip| (mip.width, mip.height, mip.rgba.as_slice()))
        }
    }

    /// Bilinear sampling within each mip, interpolated between adjacent mips.
    /// `lod` is log2 of the UV footprint measured in base-level texels.
    pub fn sample_lod(&self, uv: [f32; 2], lod: f32) -> [f32; 4] {
        if !uv[0].is_finite() || !uv[1].is_finite() {
            return [1.0; 4];
        }
        let lod = if lod.is_finite() {
            lod.clamp(0.0, self.mips.len() as f32)
        } else {
            0.0
        };
        let level = lod as usize;
        let fraction = lod - level as f32;
        let a = self.bilinear(uv, level);
        if fraction < 0.001 || level == self.mips.len() {
            return a;
        }
        let b = self.bilinear(uv, level + 1);
        let alpha = a[3] * (1.0 - fraction) + b[3] * fraction;
        let mut color = [0.0; 4];
        if alpha > 0.0 {
            for c in 0..3 {
                color[c] = (a[c] * a[3] * (1.0 - fraction) + b[c] * b[3] * fraction) / alpha;
            }
        }
        color[3] = alpha;
        color
    }

    fn bilinear(&self, uv: [f32; 2], level: usize) -> [f32; 4] {
        let (width, height, rgba) = if level == 0 {
            (self.width, self.height, self.rgba.as_slice())
        } else {
            let mip = &self.mips[level - 1];
            (mip.width, mip.height, mip.rgba.as_slice())
        };
        let u = uv[0].rem_euclid(1.0) * width as f32 - 0.5;
        let v = uv[1].rem_euclid(1.0) * height as f32 - 0.5;
        let x = u.floor() as isize;
        let y = v.floor() as isize;
        let fx = u - u.floor();
        let fy = v - v.floor();
        let mut premultiplied = [0.0; 4];
        for (dx, dy, weight) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let sx = (x + dx).rem_euclid(width as isize) as usize;
            let sy = (y + dy).rem_euclid(height as isize) as usize;
            let i = (sy * width + sx) * 4;
            let alpha = rgba[i + 3] as f32 / 255.0;
            for c in 0..3 {
                premultiplied[c] += rgba[i + c] as f32 / 255.0 * alpha * weight;
            }
            premultiplied[3] += alpha * weight;
        }
        if premultiplied[3] > 0.0 {
            for c in 0..3 {
                premultiplied[c] /= premultiplied[3];
            }
        }
        premultiplied
    }
}

fn make_mips(width: usize, height: usize, rgba: &[u8]) -> Vec<Mip> {
    let mut mips: Vec<Mip> = Vec::new();
    let (mut source_width, mut source_height) = (width, height);
    while source_width > 1 || source_height > 1 {
        let source = mips.last().map_or(rgba, |mip| mip.rgba.as_slice());
        let width = (source_width / 2).max(1);
        let height = (source_height / 2).max(1);
        let mut reduced = vec![0; width * height * 4];
        let samples_x = if source_width > 1 { 2 } else { 1 };
        let samples_y = if source_height > 1 { 2 } else { 1 };
        for y in 0..height {
            for x in 0..width {
                let mut color = [0u32; 3];
                let mut alpha = 0u32;
                for dy in 0..samples_y {
                    for dx in 0..samples_x {
                        let i = (((y * 2 + dy).min(source_height - 1) * source_width)
                            + (x * 2 + dx).min(source_width - 1))
                            * 4;
                        let a = source[i + 3] as u32;
                        for c in 0..3 {
                            color[c] += source[i + c] as u32 * a;
                        }
                        alpha += a;
                    }
                }
                let target = (y * width + x) * 4;
                for c in 0..3 {
                    reduced[target + c] = if alpha == 0 {
                        0
                    } else {
                        ((color[c] + alpha / 2) / alpha) as u8
                    };
                }
                let count = (samples_x * samples_y) as u32;
                reduced[target + 3] = ((alpha + count / 2) / count) as u8;
            }
        }
        mips.push(Mip {
            width,
            height,
            rgba: reduced,
        });
        source_width = width;
        source_height = height;
    }
    mips
}

fn relocate(pointer: u32, base: isize) -> Result<usize> {
    let value = (pointer as i64)
        .checked_add(base as i64)
        .ok_or("Texture pointer overflow")?;
    if value < 0 || value as u64 > usize::MAX as u64 {
        return Err("Texture pointer is outside its chunk".into());
    }
    Ok(value as usize)
}

fn ps2_alpha(alpha: u8) -> u8 {
    ((alpha as u16 * 255 / 128).min(255)) as u8
}

fn clut8_index(index: u8) -> usize {
    // PS2 CSM1 palette lane order interchanges bits three and four.
    ((index & !0x18) | ((index & 8) << 1) | ((index & 16) >> 1)) as usize
}

fn swizzle_index(x: usize, y: usize, log_width: u32) -> usize {
    let x = x ^ ((((y >> 1) ^ (y >> 2)) & 1) << 2);
    let nx = (x & 7) | ((x >> 1) & !7);
    let ny = (y & 1) | ((y >> 1) & !1);
    let lane = ((y >> 1) & 1) | (((x >> 3) & 1) << 1);
    lane | (nx << 2) | (ny << (log_width + 1))
}

fn unswizzle_indices(source: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    if width < 8 || height < 4 {
        return Err("Unsupported undersized swizzled PS2 texture".into());
    }
    let log_width = width.trailing_zeros();
    let slice_mask = width * 4 - 1;
    let mut out = vec![0; width * height];
    // The transfer layout repeats in four-row slices, with the lane XOR
    // retaining the original y coordinate for alternating slices.
    for y in 0..height {
        let base = (y & !3) * width;
        for x in 0..width {
            let address = base + (swizzle_index(x, y, log_width) & slice_mask);
            out[y * width + x] = *source
                .get(address)
                .ok_or("Swizzled PS2 raster is truncated")?;
        }
    }
    Ok(out)
}

fn unswizzle_rgba(source: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    // VCS's rare 32-bit rasters retain the PSP-style 16-byte by 8-row layout.
    let stride = width * 4;
    if stride < 16 || height < 8 {
        return Err("Unsupported undersized swizzled RGBA texture".into());
    }
    let blocks_across = stride / 16;
    let mut out = vec![0; source.len()];
    for y in 0..height {
        for x in 0..stride {
            let address = ((y / 8) * blocks_across + x / 16) * 128 + (y & 7) * 16 + (x & 15);
            out[y * stride + x] = *source
                .get(address)
                .ok_or("Swizzled RGBA raster is truncated")?;
        }
    }
    Ok(out)
}

/// Decode a VCS world resource or its one-level TEX_REF alias.
/// `start` is its position in `data`; `base` relocates its internal pointers.
/// Use 0 for a whole master LVZ or AREA, -32 for an IMG sector whose WRLD
/// header is stored separately in the LVZ directory. For an unsliced image,
/// use the owning chunk offset (minus 32 for slave WRLDs).
pub fn decode_raster(data: &[u8], start: usize, base: isize) -> Result<Texture> {
    let mut header = start;
    if u32le(data, header)? != 0xcccccccc {
        header = relocate(u32le(data, header)?, base)?;
    }
    bytes(data, header, 16)?;
    if u32le(data, header)? != 0xcccccccc {
        return Err("Resource is not a VCS PS2 raster".into());
    }
    let width_half = u16le(data, header + 4)? as usize;
    let format = u16le(data, header + 6)?;
    if !matches!(format & 0xff, 0x25 | 0x45) || format & 0xc000 != 0xc000 {
        return Err("Unsupported PS2 raster format".into());
    }
    let flags = u32le(data, header + 12)?;
    let log_width = flags & 0x3f;
    let log_height = (flags >> 6) & 0x3f;
    let depth = ((flags >> 12) & 0x3f) as u8;
    let mip_count = ((flags >> 20) & 0xf).max(1);
    let swizzled = flags & 0x01000000 != 0;
    if !(1..=11).contains(&log_width)
        || !(1..=11).contains(&log_height)
        || !matches!(depth, 4 | 8 | 32)
        || mip_count > 8
    {
        return Err("Invalid PS2 raster dimensions, depth or mip count".into());
    }
    let width = 1usize << log_width;
    let height = 1usize << log_height;
    if depth == 4 && width_half != 0 && width_half != width && width_half * 2 != width {
        return Err("PS2 raster width disagrees with its prefix".into());
    }
    let pixels = width * height;
    let base_bytes = pixels * depth as usize / 8;
    let mut mip_bytes = 0;
    for level in 0..mip_count {
        // RslRaster packs consecutive mip payloads, then the CLUT. Zero
        // dimensions end the chain rather than wrapping the shift count.
        mip_bytes += (width >> level) * (height >> level) * depth as usize / 8;
    }
    let palette_bytes = match depth {
        4 => 64,
        8 => 1024,
        _ => 0,
    };
    let pointer = u32le(data, header + 8)?;
    let raster = if pointer == 0 {
        header + 16
    } else {
        relocate(pointer, base)?
    };
    bytes(data, raster, mip_bytes + palette_bytes)?;
    let source = bytes(data, raster, base_bytes)?;
    let mut rgba = vec![0; pixels * 4];
    if depth == 32 {
        let linear;
        let source = if swizzled {
            linear = unswizzle_rgba(source, width, height)?;
            linear.as_slice()
        } else {
            source
        };
        rgba.copy_from_slice(source);
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = ps2_alpha(pixel[3]);
        }
    } else {
        let indices = if depth == 4 {
            let mut unpacked = Vec::with_capacity(pixels);
            for &packed in source {
                unpacked.push(packed & 15);
                unpacked.push(packed >> 4);
            }
            unpacked
        } else {
            source.to_vec()
        };
        let indices = if swizzled {
            unswizzle_indices(&indices, width, height)?
        } else {
            indices
        };
        let palette = bytes(data, raster + mip_bytes, palette_bytes)?;
        for (pixel, index) in rgba.chunks_exact_mut(4).zip(indices) {
            let index = if depth == 8 {
                clut8_index(index)
            } else {
                index as usize
            };
            pixel.copy_from_slice(&palette[index * 4..index * 4 + 4]);
            pixel[3] = ps2_alpha(pixel[3]);
        }
    }
    let has_alpha = rgba.chunks_exact(4).any(|p| p[3] != 255);
    let mips = make_mips(width, height, &rgba);
    Ok(Texture {
        width,
        height,
        rgba,
        depth,
        has_alpha,
        mips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_tint_preserves_native_mips_and_source_pixels() {
        let original = Texture {
            width: 2,
            height: 1,
            rgba: vec![255, 128, 0, 255, 64, 32, 255, 128],
            depth: 32,
            has_alpha: true,
            mips: vec![Mip {
                width: 1,
                height: 1,
                rgba: vec![20, 40, 60, 255],
            }],
        };
        let tinted = original.tinted([42, 131, 134, 130]);
        assert_eq!(tinted.rgba, [42, 66, 0, 130, 11, 16, 134, 65]);
        assert_eq!(tinted.mip(1).unwrap(), (1, 1, [3, 21, 32, 130].as_slice()));
        assert!(tinted.has_alpha);
        assert_eq!(original.rgba[0], 255);
        assert_eq!(original.mip(1).unwrap().2, [20, 40, 60, 255]);
    }

    #[test]
    fn embedded_dictionary_rejects_cycles_and_truncation() {
        assert!(Texture::load_embedded_named(&[0; 20], "particle", "water").is_err());
        let mut game = vec![0; 0x300];
        for (at, value) in [
            (0x60, 0x100u32),
            (0x100, 0x120),
            (0x108, 1),
            (0x120, 0x160),
            (0x168, 0x188),
            (0x188, 0x188),
        ] {
            game[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        game[0x128..0x130].copy_from_slice(b"particle");
        let error = Texture::load_embedded_named(&game, "particle", "water").unwrap_err();
        assert!(error.to_string().contains("links"));
        game.truncate(0x170);
        assert!(Texture::load_embedded_named(&game, "particle", "water").is_err());
    }

    fn raster(width_log: u32, height_log: u32, depth: u32, mips: u32) -> Vec<u8> {
        let width = 1usize << width_log;
        let height = 1usize << height_log;
        let mip_bytes: usize = (0..mips.max(1))
            .map(|m| (width >> m) * (height >> m) * depth as usize / 8)
            .sum();
        let palette_bytes = if depth == 4 {
            64
        } else if depth == 8 {
            1024
        } else {
            0
        };
        let mut data = vec![0; 16 + mip_bytes + palette_bytes];
        data[..4].copy_from_slice(&0xccccccccu32.to_le_bytes());
        data[4..6].copy_from_slice(&((width / 2) as u16).to_le_bytes());
        data[6..8].copy_from_slice(&0xc045u16.to_le_bytes());
        data[8..12].copy_from_slice(&16u32.to_le_bytes());
        let flags = width_log | (height_log << 6) | (depth << 12) | (mips << 20);
        data[12..16].copy_from_slice(&flags.to_le_bytes());
        data
    }

    #[test]
    fn mip_chain_palette_nibble_order_and_alpha() {
        let mut data = raster(2, 2, 4, 2);
        data[16] = 0x21;
        // 4x4 PAL4 base =8bytes;2x2 second mip =2bytes.
        data[26 + 4..26 + 8].copy_from_slice(&[255, 0, 0, 128]);
        data[26 + 8..26 + 12].copy_from_slice(&[0, 255, 0, 64]);
        let texture = decode_raster(&data, 0, 0).unwrap();
        assert_eq!(&texture.rgba[..8], &[255, 0, 0, 255, 0, 255, 0, 127]);
        assert_eq!(texture.sample([-0.75, 0.0]), [0.0, 1.0, 0.0, 127.0 / 255.0]);
        assert!(texture.has_alpha);
    }

    #[test]
    fn palette_lanes_are_swapped_independent_of_raster_swizzle() {
        let mut data = raster(2, 2, 8, 1);
        data[16] = 8;
        data[32 + 16 * 4..32 + 17 * 4].copy_from_slice(&[12, 34, 56, 128]);
        let texture = decode_raster(&data, 0, 0).unwrap();
        assert_eq!(&texture.rgba[..4], &[12, 34, 56, 255]);
    }

    #[test]
    fn sector_pointer_rebase_and_reference_alias() {
        let mut data = raster(2, 2, 4, 1);
        data[8..12].copy_from_slice(&48u32.to_le_bytes());
        assert!(decode_raster(&data, 0, -32).is_ok());
        let mut alias = vec![0; 16];
        alias[..4].copy_from_slice(&16u32.to_le_bytes());
        data[8..12].copy_from_slice(&32u32.to_le_bytes());
        alias.extend(data);
        assert!(decode_raster(&alias, 0, 0).is_ok());
    }

    #[test]
    fn swizzle_decodes_all_rows_and_preserves_indices() {
        let width = 16;
        let height = 16;
        let source: Vec<u8> = (0..width * height).map(|i| i as u8).collect();
        let decoded = unswizzle_indices(&source, width, height).unwrap();
        assert_eq!(&decoded[..8], &[0, 4, 8, 12, 16, 20, 24, 28]);
        assert_eq!(&decoded[2 * width..2 * width + 4], &[17, 21, 25, 29]);
        assert_eq!(&decoded[4 * width..4 * width + 4], &[80, 84, 88, 92]);
        let mut sorted = decoded;
        sorted.sort_unstable();
        assert_eq!(sorted, source);
    }

    #[test]
    fn rejects_truncated_and_invalid_rasters() {
        let mut data = raster(2, 2, 4, 1);
        data.pop();
        assert!(decode_raster(&data, 0, 0).is_err());
        assert!(decode_raster(&[0; 64], 0, 0).is_err());
        assert!(decode_raster(&[], usize::MAX, -32).is_err());
    }

    #[test]
    fn painted_vehicle_surface_uses_palette_and_preserves_alpha() {
        let texture = Texture {
            width: 4,
            height: 1,
            rgba: vec![
                8, 8, 8, 255, 220, 24, 20, 255, 80, 80, 80, 128, 220, 220, 220, 64,
            ],
            depth: 32,
            has_alpha: true,
            mips: Vec::new(),
        };
        let painted = texture.painted_vehicle_surface([188, 188, 180]);
        assert_eq!(&painted.rgba[..8], &[8, 8, 8, 255, 220, 24, 20, 255]);
        assert!(painted.rgba[8] > 80);
        assert!(painted.rgba[12] > painted.rgba[8]);
        assert_eq!(painted.rgba[11], 128);
        assert_eq!(painted.rgba[15], 64);
        assert_eq!(painted.mip_levels(), 3);
    }

    #[test]
    fn filtered_mips_preserve_alpha_and_avoid_transparent_color_bleed() {
        let mut data = raster(1, 1, 32, 1);
        data[16..].copy_from_slice(&[255, 0, 0, 128, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0]);
        let texture = decode_raster(&data, 0, 0).unwrap();
        assert_eq!(texture.mip_levels(), 2);
        assert_eq!(&texture.mips[0].rgba, &[255, 0, 0, 64]);
        let filtered = texture.sample_lod([0.5, 0.5], 1.0);
        assert_eq!(filtered, [1.0, 0.0, 0.0, 64.0 / 255.0]);
        let repeat = texture.sample_lod([-0.75, 0.25], 0.0);
        assert_eq!(repeat, [1.0, 0.0, 0.0, 1.0]);
    }
}
