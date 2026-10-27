//! Original VCS PS2 vehicle decoder and instance renderer.
//! Decodes native PS2 vehicle geometry, hierarchical node frames, shared wheels from GAME.DTZ,
//! and maps material textures from the streamed vehicle TXD and generic dictionary.

use crate::iso::{bytes, f32le, u16le, u32le, Disc, Result};
use crate::leeds::{Triangle, Vertex};
use crate::texture::{decode_raster, Texture};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

type Matrix = [f32; 16];
pub const VEHICLE_TEXTURE_BASE: u16 = 50000;

fn vehicle_paint_color(game: &[u8], model_id: u16) -> Result<[u8; 3]> {
    let model_infos = u32le(game, 0x3c)? as usize;
    let model_info_addr = model_infos + model_id as usize * 4;
    if model_info_addr + 4 > game.len() {
        return Ok([255, 255, 255]);
    }
    let model_info = u32le(game, model_info_addr)? as usize;
    let colour_table = u32le(game, 0x90)? as usize;
    if model_info == 0 || model_info >= game.len() || colour_table >= game.len() {
        return Ok([255, 255, 255]);
    }
    let count_byte = bytes(game, model_info + 0x161, 1)?;
    let colour_count = *count_byte.first().unwrap_or(&0) as usize;
    if colour_count == 0 || colour_count > 8 {
        return Ok([255, 255, 255]);
    }
    let colours = bytes(game, model_info + 0x163, colour_count * 2)?;
    let palette = bytes(game, colour_table, 256 * 3)?;

    let mut selected = None;
    for pair in colours.chunks_exact(2) {
        let primary_id = pair[0] as usize;
        let primary_at = (128 + primary_id) * 3;
        if let Ok(rgb) = bytes(palette, primary_at, 3) {
            let luminance = 0.2126 * rgb[0] as f32 + 0.7152 * rgb[1] as f32 + 0.0722 * rgb[2] as f32;
            if selected.is_none_or(|(_, best)| luminance > best) {
                selected = Some(([rgb[0], rgb[1], rgb[2]], luminance));
            }
        }
    }
    Ok(selected.map(|(rgb, _)| rgb).unwrap_or([255, 255, 255]))
}


#[derive(Serialize, Clone, Debug)]
pub struct VehicleStats {
    pub name: String,
    pub model_id: u16,
    pub txd_id: u16,
    pub wheel_model_id: u16,
    pub wheel_scale: f32,
    pub total_atomics: usize,
    pub visible_atomics: usize,
    pub body_triangles: usize,
    pub wheel_triangles: usize,
    pub total_triangles: usize,
    pub bounds: [[f32; 3]; 2],
    pub texture_names: Vec<String>,
    pub resolved_texture_names: Vec<String>,
    pub missing_texture_names: Vec<String>,
    pub handling_record_address: usize,
    pub textures: Vec<VehicleTextureInfo>,
    pub collision_primitives: [usize; 2],
    pub frame_positions: std::collections::BTreeMap<String, [f32; 3]>,
}

#[derive(Serialize, Clone, Debug)]
pub struct VehicleTextureInfo {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub has_alpha: bool,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct VehicleModel {
    pub stats: VehicleStats,
    pub triangles: Vec<Triangle>,
    pub textures: HashMap<u16, Texture>,
    pub bounds: [[f32; 3]; 2],
    pub wheels: Vec<VehicleWheel>,
    pub doors: Vec<VehicleDoor>,
    pub collision: crate::collision::NativeCollider,
}

#[derive(Clone, Debug)]
pub struct VehicleWheel {
    pub triangle_range: std::ops::Range<usize>,
    pub center: [f32; 3],
    pub front: bool,
    pub is_left: bool,
}

#[derive(Clone, Debug)]
pub struct VehicleDoor {
    pub triangle_range: std::ops::Range<usize>,
    pub hinge: [f32; 3],
    pub side: f32,
}

#[derive(Clone)]
struct FrameNode {
    name: String,
    world: Matrix,
}

#[allow(dead_code)]
#[derive(Clone)]
struct RawVertex {
    p: [f32; 3],
    uv: [f32; 2],
    color: [f32; 3],
    material: usize,
}

fn identity() -> Matrix {
    let mut m = [0.0; 16];
    for i in 0..4 {
        m[i * 5] = 1.0;
    }
    m
}

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            for k in 0..4 {
                out[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
            }
        }
    }
    out
}

fn transform(m: Matrix, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn matrix(data: &[u8], at: usize) -> Result<Matrix> {
    bytes(data, at, 64)?;
    let mut m = [0.0; 16];
    for i in 0..16 {
        m[i] = f32le(data, at + i * 4)?;
    }
    m[3] = 0.0;
    m[7] = 0.0;
    m[11] = 0.0;
    m[15] = 1.0;
    if m.iter().any(|x| !x.is_finite()) {
        return Err("Vehicle matrix contains non-finite values".into());
    }
    Ok(m)
}

fn string(data: &[u8], at: usize, max: usize) -> Result<String> {
    let end_limit = (at + max).min(data.len());
    let slice = &data[at..end_limit];
    let len = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    Ok(std::str::from_utf8(&slice[..len])?.to_string())
}

fn unpack(data: &[u8], at: usize, expected: u32) -> Result<(usize, usize, usize)> {
    let code = u32le(data, at)?;
    if code & 0xff004000 != expected {
        return Err(format!("Unexpected vehicle VIF opcode {code:#x} at {at:#x}").into());
    }
    let command = (code >> 24) as usize;
    let count = (((code >> 16) & 255) as usize).max(1);
    let count = if ((code >> 16) & 255) == 0 {
        256
    } else {
        count
    };
    let length = if command & 3 == 3 {
        count * 2
    } else {
        count * (((command >> 2) & 3) + 1) * [4, 2, 1][command & 3]
    };
    bytes(data, at + 4, length)?;
    Ok((at + 4, (at + 4 + length + 3) & !3, count))
}

fn walk_frames(
    data: &[u8],
    node: usize,
    parent_world: Option<Matrix>,
    frames: &mut HashMap<usize, FrameNode>,
    seen: &mut HashSet<usize>,
) -> Result<()> {
    if frames.len() >= 256 || !seen.insert(node) {
        return Err("Cyclic or excessively deep vehicle frame hierarchy".into());
    }
    bytes(data, node, 0xb0)?;
    let local = matrix(data, node + 0x10)?;
    let world = if let Some(parent) = parent_world {
        multiply(parent, local)
    } else {
        local
    };
    let name_ptr = u32le(data, node + 0xa8)? as usize;
    let name = if name_ptr != 0 && name_ptr < data.len() {
        string(data, name_ptr, 64)?
    } else {
        String::new()
    };
    frames.insert(node, FrameNode { name, world });
    let mut child = u32le(data, node + 0x90)? as usize;
    while child != 0 {
        walk_frames(data, child, Some(world), frames, seen)?;
        bytes(data, child, 0x98)?;
        child = u32le(data, child + 0x94)? as usize;
    }
    Ok(())
}

fn decode_atomic_triangles(
    data: &[u8],
    geometry: usize,
    world: Matrix,
    material_colors: &[[f32; 3]],
    material_texture_indices: &[u16],
    out_triangles: &mut Vec<Triangle>,
) -> Result<()> {
    let h = geometry + 0x20;
    let _flags = u32le(data, h + 20)?;
    let strips = (u32le(data, h + 16)? >> 20) as usize;
    if strips == 0 || strips > 512 {
        return Err("Invalid strip count in vehicle geometry".into());
    }
    let scale = [
        f32le(data, h + 40)?,
        f32le(data, h + 44)?,
        f32le(data, h + 48)?,
    ];
    let origin = [
        f32le(data, h + 52)?,
        f32le(data, h + 56)?,
        f32le(data, h + 60)?,
    ];
    let strip_table = h + 64;
    bytes(data, strip_table, strips * 48)?;
    let dma_base = strip_table + strips * 48;

    let mut verts = Vec::new();
    for si in 0..strips {
        let row = strip_table + si * 48;
        let uv_scale = [f32le(data, row + 16)?, f32le(data, row + 20)?];
        let nt = u16le(data, row + 32)? as usize;
        let mat = u16le(data, row + 34)? as usize;
        let packet = dma_base + u32le(data, row + 28)? as usize;
        let length = ((u32le(data, packet)? & 0xffff) as usize + 1) * 16;
        bytes(data, packet, length)?;
        let end = packet + length;
        let mut cursor = packet + 16;
        let start = verts.len();
        let mut batch = 0;

        while cursor < end {
            if u32le(data, cursor)? != 0x6c018000 {
                return Err("Unexpected VIF tag in vehicle packet".into());
            }
            let count = (u32le(data, cursor + 16)? & 0x7fff) as usize;
            cursor += 20;
            if u32le(data, cursor)? != 0x20000000 || u32le(data, cursor + 8)? != 0x30000000 {
                return Err("Missing VIF direct markers in vehicle packet".into());
            }
            let (vp, next_pos, _) = unpack(data, cursor + 28, 0x79000000)?;
            cursor = next_pos;
            if u32le(data, cursor)? != 0x20000000 || u32le(data, cursor + 8)? != 0x30000000 {
                return Err("Missing UV direct markers in vehicle packet".into());
            }
            let (uvp, next_uv, _) = unpack(data, cursor + 28, 0x76004000)?;
            cursor = next_uv;
            let (_, next_n, _) = unpack(data, cursor, 0x6a000000)?;
            cursor = next_n;
            if u32le(data, cursor)? != 0x14000006 {
                return Err("Missing final VIF marker in vehicle batch".into());
            }
            cursor += 4;
            while cursor < end && u32le(data, cursor)? == 0 {
                cursor += 4;
            }

            let mat_color = material_colors.get(mat).copied().unwrap_or([1.0, 1.0, 1.0]);
            for v in (if batch == 0 { 0 } else { 2 })..count {
                let rx = (i16::from_le_bytes(bytes(data, vp + v * 6, 2)?.try_into()?) as f32)
                    / 32767.5
                    * scale[0]
                    + origin[0];
                let ry = (i16::from_le_bytes(bytes(data, vp + v * 6 + 2, 2)?.try_into()?) as f32)
                    / 32767.5
                    * scale[1]
                    + origin[1];
                let rz = (i16::from_le_bytes(bytes(data, vp + v * 6 + 4, 2)?.try_into()?) as f32)
                    / 32767.5
                    * scale[2]
                    + origin[2];
                let p = transform(world, [rx, ry, rz]);

                let u = (data[uvp + v * 2] as f32 / 127.5) * uv_scale[0];
                let v_coord = (data[uvp + v * 2 + 1] as f32 / 127.5) * uv_scale[1];

                verts.push(RawVertex {
                    p,
                    uv: [u, v_coord],
                    color: mat_color,
                    material: mat,
                });
            }
            batch += 1;
        }

        let tex_id = material_texture_indices
            .get(mat)
            .copied()
            .unwrap_or(VEHICLE_TEXTURE_BASE);

        for k in 2..(nt + 2) {
            let (i0, i1, i2) = if k % 2 == 0 {
                (start + k - 2, start + k - 1, start + k)
            } else {
                (start + k - 1, start + k - 2, start + k)
            };
            if i2 >= verts.len() {
                break;
            }
            let v0 = &verts[i0];
            let v1 = &verts[i1];
            let v2 = &verts[i2];
            let e = [v1.p[0] - v0.p[0], v1.p[1] - v0.p[1], v1.p[2] - v0.p[2]];
            let f = [v2.p[0] - v0.p[0], v2.p[1] - v0.p[1], v2.p[2] - v0.p[2]];
            let cross = [
                e[1] * f[2] - e[2] * f[1],
                e[2] * f[0] - e[0] * f[2],
                e[0] * f[1] - e[1] * f[0],
            ];
            let area2 = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
            if area2 > 1e-12 {
                out_triangles.push(Triangle {
                    vertices: [
                        Vertex {
                            p: v0.p,
                            color: v0.color,
                            uv: v0.uv,
                        },
                        Vertex {
                            p: v1.p,
                            color: v1.color,
                            uv: v1.uv,
                        },
                        Vertex {
                            p: v2.p,
                            color: v2.color,
                            uv: v2.uv,
                        },
                    ],
                    texture_id: tex_id,
                });
            }
        }
    }
    Ok(())
}

fn load_streamed_txd(data: &[u8]) -> Result<HashMap<String, Texture>> {
    let mut out = HashMap::new();
    if bytes(data, 0, 4)? != b"xet\0" {
        return Err("Invalid TXD magic".into());
    }
    let head = 0x28;
    let mut link = u32le(data, head)? as usize;
    let mut seen = HashSet::new();
    while link != head {
        if link < 8 || seen.len() >= 256 || !seen.insert(link) {
            return Err("Cyclic or oversized streamed TXD".into());
        }
        let texture = link - 8;
        let name = string(data, texture + 16, 32)?;
        let raster = u32le(data, texture)? as usize;
        bytes(data, raster, 16)?;
        let format = u16le(data, raster + 6)?;
        if u32le(data, raster)? == 0 && matches!(format & 0xff, 0x25 | 0x45) {
            let mut canonical = data.to_vec();
            canonical[raster..raster + 4].copy_from_slice(&0xccccccccu32.to_le_bytes());
            canonical[raster + 6..raster + 8].copy_from_slice(&(format | 0xc000).to_le_bytes());
            if let Ok(tex) = decode_raster(&canonical, raster, 0) {
                out.insert(name.to_lowercase(), tex);
            }
        }
        link = u32le(data, link)? as usize;
    }
    Ok(out)
}

fn load_embedded_generic_txd(game: &[u8]) -> Result<HashMap<String, Texture>> {
    let mut out = HashMap::new();
    let txd_pool = u32le(game, 0x60)? as usize;
    let txd_items = u32le(game, txd_pool)? as usize;
    let txd_count = u32le(game, txd_pool + 8)? as usize;
    for slot in 0..txd_count.min(16) {
        let name = string(game, txd_items + slot * 28 + 8, 20)?;
        if name.eq_ignore_ascii_case("generic") {
            let dictionary = u32le(game, txd_items + slot * 28)? as usize;
            if dictionary == 0 {
                continue;
            }
            let head = dictionary + 8;
            let mut link = u32le(game, head)? as usize;
            let mut seen = HashSet::new();
            while link != head {
                if link < 8 || seen.len() >= 512 || !seen.insert(link) {
                    break;
                }
                let texture = link - 8;
                let tex_name = string(game, texture + 16, 32)?;
                let raster = u32le(game, texture)? as usize;
                if raster != 0 && raster + 16 <= game.len() {
                    let format = u16le(game, raster + 6)?;
                    let mut canonical = game.to_vec();
                    canonical[raster..raster + 4].copy_from_slice(&0xccccccccu32.to_le_bytes());
                    canonical[raster + 6..raster + 8]
                        .copy_from_slice(&(format | 0xc000).to_le_bytes());
                    if let Ok(tex) = decode_raster(&canonical, raster, 0) {
                        out.insert(tex_name.to_lowercase(), tex);
                    }
                }
                link = u32le(game, link)? as usize;
            }
        }
    }
    Ok(out)
}


/// Vehicle types supported in GTA Vice City Stories.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum VehicleType {
    /// Standard 4-wheeled automobiles, trucks, vans, and tracked tanks
    Automobile,
    /// Surface watercraft (speedboats, cruisers, yachts, cutters, dinghies)
    Boat,
    /// Personal watercraft with jet nozzle vectoring (Jetski)
    Jetski,
    /// Rotary aircraft (Maverick, Little Willie, Sea Sparrow, Hunter)
    Helicopter,
    /// Fixed-wing aircraft and air-cushioned hovercraft (Biplane, Bovver '64)
    PlaneOrHover,
    /// Two-wheeled motorized cycles (Sanchez, PCJ-600, Faggio, Angel, Freeway)
    Motorcycle,
    /// Pedal bicycles (BMX, BMX Gold)
    Bicycle,
    /// Four-wheeled all-terrain quad cycle
    QuadBike,
}

/// Metadata and technical parameters for a vehicle model entry in GAME.DTZ.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize)]
pub struct VehicleCatalogEntry {
    pub id: u16,
    pub game_name: &'static str,
    pub display_name: &'static str,
    pub vehicle_type: VehicleType,
    pub wheel_model_id: u16,
    pub wheel_scale: f32,
    pub wheel_scale_rear: f32,
    pub txd_id: u16,
    pub handling_address: usize,
}

#[allow(dead_code)]
impl VehicleCatalogEntry {
    #[inline]
    pub fn is_watercraft(&self) -> bool {
        matches!(self.vehicle_type, VehicleType::Boat | VehicleType::Jetski)
            || self.game_name == "HOVER"
            || self.game_name == "SKIMMER"
            || self.game_name == "SEASPAR"
    }

    #[inline]
    pub fn is_aircraft(&self) -> bool {
        matches!(self.vehicle_type, VehicleType::Helicopter | VehicleType::PlaneOrHover)
    }

    #[inline]
    pub fn is_two_wheeler(&self) -> bool {
        matches!(self.vehicle_type, VehicleType::Motorcycle | VehicleType::Bicycle)
    }

    #[inline]
    pub fn is_amphibious(&self) -> bool {
        self.game_name == "HOVER" || self.game_name == "SKIMMER" || self.game_name == "SEASPAR"
    }

    #[inline]
    pub fn has_wheels(&self) -> bool {
        self.wheel_model_id != 65535 && self.wheel_model_id != 0
    }
}

/// Query and catalog lookup interface for all 111 Vice City Stories vehicles.
#[allow(dead_code)]
pub struct VehicleCatalog;

#[allow(dead_code)]
impl VehicleCatalog {
    /// Returns the complete immutable array of all 111 vehicles.
    pub fn all() -> &'static [VehicleCatalogEntry] {
        VEHICLE_CATALOG
    }

    /// Lookup a vehicle by model ID (170..=280).
    pub fn get(id: u16) -> Option<&'static VehicleCatalogEntry> {
        VEHICLE_CATALOG.iter().find(|e| e.id == id)
    }

    /// Case-insensitive search by game name (e.g. "BANSHEE", "SQUALO") or display name ("Squalo Speedboat").
    pub fn find(name: &str) -> Option<&'static VehicleCatalogEntry> {
        let trimmed = name.trim();
        let upper = trimmed.to_uppercase();
        VEHICLE_CATALOG.iter().find(|e| {
            e.game_name == upper
                || e.display_name.eq_ignore_ascii_case(trimmed)
        })
    }

    /// Returns all surface watercraft and amphibious vessels.
    pub fn watercraft() -> Vec<&'static VehicleCatalogEntry> {
        VEHICLE_CATALOG.iter().filter(|e| e.is_watercraft()).collect()
    }

    /// Returns all 4-wheeled road automobiles, trucks, and heavy vehicles.
    pub fn automobiles() -> Vec<&'static VehicleCatalogEntry> {
        VEHICLE_CATALOG
            .iter()
            .filter(|e| e.vehicle_type == VehicleType::Automobile)
            .collect()
    }

    /// Returns all motorcycles, mopeds, and bicycles.
    pub fn motorcycles() -> Vec<&'static VehicleCatalogEntry> {
        VEHICLE_CATALOG.iter().filter(|e| e.is_two_wheeler()).collect()
    }

    /// Returns all helicopters and airplanes.
    pub fn aircraft() -> Vec<&'static VehicleCatalogEntry> {
        VEHICLE_CATALOG.iter().filter(|e| e.is_aircraft()).collect()
    }
}

pub const VEHICLE_CATALOG: &[VehicleCatalogEntry] = &[
    VehicleCatalogEntry { id: 170, game_name: "6WEELAT", display_name: "6-Wheel ATV", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 166, handling_address: 0x4c52a0 },
    VehicleCatalogEntry { id: 171, game_name: "ADMIRAL", display_name: "Admiral", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 167, handling_address: 0x4c5380 },
    VehicleCatalogEntry { id: 172, game_name: "CHEETAH", display_name: "Cheetah", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 168, handling_address: 0x4c5460 },
    VehicleCatalogEntry { id: 173, game_name: "AUTOGYR", display_name: "Little Willie Autogyro", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 169, handling_address: 0x4c5540 },
    VehicleCatalogEntry { id: 174, game_name: "BAGGAGE", display_name: "Baggage Handler", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.60, wheel_scale_rear: 0.60, txd_id: 170, handling_address: 0x4c5620 },
    VehicleCatalogEntry { id: 175, game_name: "BANSHEE", display_name: "Banshee", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 171, handling_address: 0x4c5700 },
    VehicleCatalogEntry { id: 176, game_name: "PEREN", display_name: "Perennial", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.66, wheel_scale_rear: 0.66, txd_id: 172, handling_address: 0x4c57e0 },
    VehicleCatalogEntry { id: 177, game_name: "BLISTA", display_name: "Blista Compact", vehicle_type: VehicleType::Automobile, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 173, handling_address: 0x4c58c0 },
    VehicleCatalogEntry { id: 178, game_name: "BMX", display_name: "BMX", vehicle_type: VehicleType::Bicycle, wheel_model_id: 65535, wheel_scale: 0.60, wheel_scale_rear: 0.60, txd_id: 174, handling_address: 0x4c4990 },
    VehicleCatalogEntry { id: 179, game_name: "BMXG", display_name: "BMX (Gold)", vehicle_type: VehicleType::Bicycle, wheel_model_id: 65535, wheel_scale: 0.60, wheel_scale_rear: 0.90, txd_id: 175, handling_address: 0x4c4a70 },
    VehicleCatalogEntry { id: 180, game_name: "BOBCAT", display_name: "Bobcat", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 176, handling_address: 0x4c4b50 },
    VehicleCatalogEntry { id: 181, game_name: "BULLDOZ", display_name: "Bulldozer", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.40, wheel_scale_rear: 1.40, txd_id: 177, handling_address: 0x4c4c30 },
    VehicleCatalogEntry { id: 182, game_name: "BURRITO", display_name: "Burrito", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 178, handling_address: 0x4c4d10 },
    VehicleCatalogEntry { id: 183, game_name: "CABBIE", display_name: "Cabbie", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 179, handling_address: 0x4c4df0 },
    VehicleCatalogEntry { id: 184, game_name: "CADDY", display_name: "Caddy", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.50, wheel_scale_rear: 0.50, txd_id: 180, handling_address: 0x4c4ed0 },
    VehicleCatalogEntry { id: 185, game_name: "SPEEDER2", display_name: "Speeder II", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 181, handling_address: 0x4c4fb0 },
    VehicleCatalogEntry { id: 186, game_name: "PIMP", display_name: "Polaroid Pimp Wagon", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.80, wheel_scale_rear: 0.85, txd_id: 182, handling_address: 0x4c5090 },
    VehicleCatalogEntry { id: 187, game_name: "DELUXO", display_name: "Deluxo", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 183, handling_address: 0x4c41b0 },
    VehicleCatalogEntry { id: 188, game_name: "HUEY", display_name: "Armadillo / Huey", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 184, handling_address: 0x4c4290 },
    VehicleCatalogEntry { id: 189, game_name: "AIRAMB", display_name: "Air Ambulance", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 185, handling_address: 0x4c4370 },
    VehicleCatalogEntry { id: 190, game_name: "ELECTRA", display_name: "WinterGreen / Electra", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.65, wheel_scale_rear: 0.65, txd_id: 186, handling_address: 0x4c4450 },
    VehicleCatalogEntry { id: 191, game_name: "ELECTRP", display_name: "Police WinterGreen", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.65, wheel_scale_rear: 0.65, txd_id: 187, handling_address: 0x4c4530 },
    VehicleCatalogEntry { id: 192, game_name: "ESPERAN", display_name: "Esperanto", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.64, wheel_scale_rear: 0.64, txd_id: 188, handling_address: 0x4c4610 },
    VehicleCatalogEntry { id: 193, game_name: "FBICAR", display_name: "FBI Washington", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 189, handling_address: 0x4c46f0 },
    VehicleCatalogEntry { id: 194, game_name: "FIRETRK", display_name: "Fire Truck", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.00, wheel_scale_rear: 1.00, txd_id: 190, handling_address: 0x4c47d0 },
    VehicleCatalogEntry { id: 195, game_name: "GLENDAL", display_name: "Glendale", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 191, handling_address: 0x4c48b0 },
    VehicleCatalogEntry { id: 196, game_name: "GREENWO", display_name: "Greenwood", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 192, handling_address: 0x4c39d0 },
    VehicleCatalogEntry { id: 197, game_name: "HERMES", display_name: "Hermes", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 193, handling_address: 0x4c3ab0 },
    VehicleCatalogEntry { id: 198, game_name: "HOVER", display_name: "Bovver '64 Hovercraft", vehicle_type: VehicleType::PlaneOrHover, wheel_model_id: 329, wheel_scale: 1.00, wheel_scale_rear: 1.00, txd_id: 194, handling_address: 0x4c3b90 },
    VehicleCatalogEntry { id: 199, game_name: "IDAHO", display_name: "Idaho", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.64, wheel_scale_rear: 0.64, txd_id: 195, handling_address: 0x4c3c70 },
    VehicleCatalogEntry { id: 200, game_name: "LANDSTK", display_name: "Landstalker", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.82, wheel_scale_rear: 0.95, txd_id: 196, handling_address: 0x4c3d50 },
    VehicleCatalogEntry { id: 201, game_name: "MANANA", display_name: "Manana", vehicle_type: VehicleType::Automobile, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 197, handling_address: 0x4c3e30 },
    VehicleCatalogEntry { id: 202, game_name: "MOPED", display_name: "Faggio Moped", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.46, wheel_scale_rear: 0.46, txd_id: 198, handling_address: 0x4c3f10 },
    VehicleCatalogEntry { id: 203, game_name: "OCEANIC", display_name: "Oceanic", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 199, handling_address: 0x4c3ff0 },
    VehicleCatalogEntry { id: 204, game_name: "VICECHE", display_name: "Vice Cheetah", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 200, handling_address: 0x4c40d0 },
    VehicleCatalogEntry { id: 205, game_name: "SANCHEZ", display_name: "Sanchez Dirt Bike", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.66, wheel_scale_rear: 0.66, txd_id: 201, handling_address: 0x4c31f0 },
    VehicleCatalogEntry { id: 206, game_name: "STALION", display_name: "Stallion", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 202, handling_address: 0x4c32d0 },
    VehicleCatalogEntry { id: 207, game_name: "POLICEM", display_name: "VCPD Police Cruiser", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 203, handling_address: 0x4c33b0 },
    VehicleCatalogEntry { id: 208, game_name: "BOBO", display_name: "Bobo Delivery Truck", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.30, wheel_scale_rear: 0.30, txd_id: 204, handling_address: 0x4c3490 },
    VehicleCatalogEntry { id: 209, game_name: "PATRIOT", display_name: "Patriot", vehicle_type: VehicleType::Automobile, wheel_model_id: 329, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 205, handling_address: 0x4c3570 },
    VehicleCatalogEntry { id: 210, game_name: "PONY", display_name: "Pony", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 206, handling_address: 0x4c3650 },
    VehicleCatalogEntry { id: 211, game_name: "SENTINL", display_name: "Sentinel", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 207, handling_address: 0x4c3730 },
    VehicleCatalogEntry { id: 212, game_name: "PCJ600", display_name: "PCJ-600", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.67, wheel_scale_rear: 0.67, txd_id: 208, handling_address: 0x4c3810 },
    VehicleCatalogEntry { id: 213, game_name: "MAVERIC", display_name: "Maverick", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 209, handling_address: 0x4c38f0 },
    VehicleCatalogEntry { id: 214, game_name: "REEFER", display_name: "Reefer Trawler", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 210, handling_address: 0x4c2a10 },
    VehicleCatalogEntry { id: 215, game_name: "SPEEDER", display_name: "Speeder", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 211, handling_address: 0x4c2af0 },
    VehicleCatalogEntry { id: 216, game_name: "LINERUN", display_name: "Line Runner", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.10, wheel_scale_rear: 1.10, txd_id: 212, handling_address: 0x4c2bd0 },
    VehicleCatalogEntry { id: 217, game_name: "WALTON", display_name: "Walton", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 213, handling_address: 0x4c2cb0 },
    VehicleCatalogEntry { id: 218, game_name: "BARRCKS", display_name: "Barracks OL", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.20, wheel_scale_rear: 1.20, txd_id: 214, handling_address: 0x4c2d90 },
    VehicleCatalogEntry { id: 219, game_name: "PREDATR", display_name: "Predator Police Boat", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 215, handling_address: 0x4c2e70 },
    VehicleCatalogEntry { id: 220, game_name: "FLATBED", display_name: "Flatbed", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.20, wheel_scale_rear: 1.20, txd_id: 216, handling_address: 0x4c2f50 },
    VehicleCatalogEntry { id: 221, game_name: "BARRACK", display_name: "Barracks OL Flat", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.20, wheel_scale_rear: 1.20, txd_id: 217, handling_address: 0x4c3030 },
    VehicleCatalogEntry { id: 222, game_name: "BIPLANE", display_name: "Biplane", vehicle_type: VehicleType::PlaneOrHover, wheel_model_id: 325, wheel_scale: 1.00, wheel_scale_rear: 1.00, txd_id: 218, handling_address: 0x4c3110 },
    VehicleCatalogEntry { id: 223, game_name: "MOONBM", display_name: "Moonbeam", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 219, handling_address: 0x4c1aa0 },
    VehicleCatalogEntry { id: 224, game_name: "RUMPO", display_name: "Rumpo", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 220, handling_address: 0x4c1b80 },
    VehicleCatalogEntry { id: 225, game_name: "YOLA", display_name: "Yola Dinghy", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 221, handling_address: 0x4c1c60 },
    VehicleCatalogEntry { id: 226, game_name: "TAXI", display_name: "Taxi", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 222, handling_address: 0x4c1d40 },
    VehicleCatalogEntry { id: 227, game_name: "AMBULAN", display_name: "Ambulance", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 223, handling_address: 0x4c1e20 },
    VehicleCatalogEntry { id: 228, game_name: "STRETCH", display_name: "Stretch Limousine", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.75, wheel_scale_rear: 0.75, txd_id: 224, handling_address: 0x4c1f00 },
    VehicleCatalogEntry { id: 229, game_name: "FAGGIO", display_name: "Faggio", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.46, wheel_scale_rear: 0.46, txd_id: 225, handling_address: 0x4c1fe0 },
    VehicleCatalogEntry { id: 230, game_name: "QUADBIK", display_name: "Quad Bike", vehicle_type: VehicleType::QuadBike, wheel_model_id: 327, wheel_scale: 0.76, wheel_scale_rear: 0.76, txd_id: 226, handling_address: 0x4c20c0 },
    VehicleCatalogEntry { id: 231, game_name: "ANGEL", display_name: "Angel Chopper", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.65, wheel_scale_rear: 0.65, txd_id: 227, handling_address: 0x4c21a0 },
    VehicleCatalogEntry { id: 232, game_name: "FREEWAY", display_name: "Freeway", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.65, wheel_scale_rear: 0.65, txd_id: 228, handling_address: 0x4c12c0 },
    VehicleCatalogEntry { id: 233, game_name: "JETSKI", display_name: "Jetski", vehicle_type: VehicleType::Jetski, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 229, handling_address: 0x4c13a0 },
    VehicleCatalogEntry { id: 234, game_name: "ENFORCE", display_name: "Enforcer SWAT Truck", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 230, handling_address: 0x4c1480 },
    VehicleCatalogEntry { id: 235, game_name: "BOXVIL", display_name: "Boxville", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 231, handling_address: 0x4c1560 },
    VehicleCatalogEntry { id: 236, game_name: "BENSON", display_name: "Benson", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 232, handling_address: 0x4c1640 },
    VehicleCatalogEntry { id: 237, game_name: "COACH", display_name: "Coach Bus", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.00, wheel_scale_rear: 1.00, txd_id: 233, handling_address: 0x4c1720 },
    VehicleCatalogEntry { id: 238, game_name: "MULE", display_name: "Mule", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 0.76, wheel_scale_rear: 0.76, txd_id: 234, handling_address: 0x4c1800 },
    VehicleCatalogEntry { id: 239, game_name: "VOODOO", display_name: "Voodoo Lowrider", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 235, handling_address: 0x4c18e0 },
    VehicleCatalogEntry { id: 240, game_name: "SECURI", display_name: "Securicar", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 0.85, wheel_scale_rear: 0.85, txd_id: 236, handling_address: 0x4c19c0 },
    VehicleCatalogEntry { id: 241, game_name: "TRASHM", display_name: "Trashmaster", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.06, wheel_scale_rear: 1.06, txd_id: 237, handling_address: 0x4c0ae0 },
    VehicleCatalogEntry { id: 242, game_name: "TOPFUN", display_name: "Top Fun Van", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 238, handling_address: 0x4c0bc0 },
    VehicleCatalogEntry { id: 243, game_name: "YANKEE", display_name: "Yankee", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 0.84, wheel_scale_rear: 0.84, txd_id: 239, handling_address: 0x4c0ca0 },
    VehicleCatalogEntry { id: 244, game_name: "WHOOPEE", display_name: "Mr. Whoopee Ice Cream Van", vehicle_type: VehicleType::Automobile, wheel_model_id: 325, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 240, handling_address: 0x4c0d80 },
    VehicleCatalogEntry { id: 245, game_name: "SANDKIN", display_name: "Sandking", vehicle_type: VehicleType::Automobile, wheel_model_id: 329, wheel_scale: 0.92, wheel_scale_rear: 0.92, txd_id: 241, handling_address: 0x4c0e60 },
    VehicleCatalogEntry { id: 246, game_name: "RHINO", display_name: "Rhino Tank", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 1.50, wheel_scale_rear: 1.50, txd_id: 242, handling_address: 0x4c0f40 },
    VehicleCatalogEntry { id: 247, game_name: "DINGHY", display_name: "Dinghy", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 243, handling_address: 0x4c1020 },
    VehicleCatalogEntry { id: 248, game_name: "MARQUIS", display_name: "Marquis Sailing Yacht", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 244, handling_address: 0x4c1100 },
    VehicleCatalogEntry { id: 249, game_name: "RIO", display_name: "Rio Yacht", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 245, handling_address: 0x4c11e0 },
    VehicleCatalogEntry { id: 250, game_name: "TROPIC", display_name: "Tropic Cruiser", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 246, handling_address: 0x4c0300 },
    VehicleCatalogEntry { id: 251, game_name: "FORKLIF", display_name: "Forklift", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.50, wheel_scale_rear: 0.50, txd_id: 247, handling_address: 0x4c03e0 },
    VehicleCatalogEntry { id: 252, game_name: "STREETF", display_name: "Streetfighter", vehicle_type: VehicleType::Motorcycle, wheel_model_id: 65535, wheel_scale: 0.66, wheel_scale_rear: 0.66, txd_id: 248, handling_address: 0x4c04c0 },
    VehicleCatalogEntry { id: 253, game_name: "VIRGO", display_name: "Virgo", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.66, wheel_scale_rear: 0.66, txd_id: 249, handling_address: 0x4c05a0 },
    VehicleCatalogEntry { id: 254, game_name: "STINGER", display_name: "Stinger", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 250, handling_address: 0x4c0680 },
    VehicleCatalogEntry { id: 255, game_name: "BFINJC", display_name: "BF Injection", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 251, handling_address: 0x4c0760 },
    VehicleCatalogEntry { id: 256, game_name: "PHEONIX", display_name: "Phoenix", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 252, handling_address: 0x4c0840 },
    VehicleCatalogEntry { id: 257, game_name: "SQUALO", display_name: "Squalo Speedboat", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 253, handling_address: 0x4c0920 },
    VehicleCatalogEntry { id: 258, game_name: "JETMAX", display_name: "Jetmax Racing Boat", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 254, handling_address: 0x4c0a00 },
    VehicleCatalogEntry { id: 259, game_name: "MESA", display_name: "Mesa Grande", vehicle_type: VehicleType::Automobile, wheel_model_id: 328, wheel_scale: 0.90, wheel_scale_rear: 0.90, txd_id: 255, handling_address: 0x4bfb20 },
    VehicleCatalogEntry { id: 260, game_name: "VCNMAV", display_name: "VCN News Maverick", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 256, handling_address: 0x4bfc00 },
    VehicleCatalogEntry { id: 261, game_name: "POLMAV", display_name: "VCPD Police Maverick", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 257, handling_address: 0x4bfce0 },
    VehicleCatalogEntry { id: 262, game_name: "SPARROW", display_name: "Sparrow", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 258, handling_address: 0x4bfdc0 },
    VehicleCatalogEntry { id: 263, game_name: "SEASPAR", display_name: "Sea Sparrow", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.64, txd_id: 259, handling_address: 0x4bfea0 },
    VehicleCatalogEntry { id: 264, game_name: "SCARAB", display_name: "Scarab Offshore Boat", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 260, handling_address: 0x4bff80 },
    VehicleCatalogEntry { id: 265, game_name: "CHOLO", display_name: "Cholo Sabre", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 261, handling_address: 0x4c0060 },
    VehicleCatalogEntry { id: 266, game_name: "COMET", display_name: "Comet", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 262, handling_address: 0x4c0140 },
    VehicleCatalogEntry { id: 267, game_name: "CUBAN", display_name: "Cuban Hermes", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 263, handling_address: 0x4c0220 },
    VehicleCatalogEntry { id: 268, game_name: "FBIRAN", display_name: "FBI Rancher", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 1.10, wheel_scale_rear: 1.10, txd_id: 264, handling_address: 0x4bf340 },
    VehicleCatalogEntry { id: 269, game_name: "GANGBUR", display_name: "Gang Burrito", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 1.09, wheel_scale_rear: 1.09, txd_id: 265, handling_address: 0x4bf420 },
    VehicleCatalogEntry { id: 270, game_name: "INFERNS", display_name: "Infernus", vehicle_type: VehicleType::Automobile, wheel_model_id: 323, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 266, handling_address: 0x4bf500 },
    VehicleCatalogEntry { id: 271, game_name: "REGINA", display_name: "Regina Station Wagon", vehicle_type: VehicleType::Automobile, wheel_model_id: 326, wheel_scale: 0.66, wheel_scale_rear: 0.66, txd_id: 267, handling_address: 0x4bf5e0 },
    VehicleCatalogEntry { id: 272, game_name: "SABRE", display_name: "Sabre", vehicle_type: VehicleType::Automobile, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 268, handling_address: 0x4bf6c0 },
    VehicleCatalogEntry { id: 273, game_name: "SABRETB", display_name: "Sabre Turbo", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 269, handling_address: 0x4bf7a0 },
    VehicleCatalogEntry { id: 274, game_name: "SENTXS", display_name: "Sentinel XS", vehicle_type: VehicleType::Automobile, wheel_model_id: 327, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 270, handling_address: 0x4bf880 },
    VehicleCatalogEntry { id: 275, game_name: "HUNTER", display_name: "Hunter Attack Helicopter", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.64, wheel_scale_rear: 0.64, txd_id: 271, handling_address: 0x4bf960 },
    VehicleCatalogEntry { id: 276, game_name: "WASHIN", display_name: "Washington", vehicle_type: VehicleType::Automobile, wheel_model_id: 324, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 272, handling_address: 0x4bfa40 },
    VehicleCatalogEntry { id: 277, game_name: "COASTG", display_name: "Coast Guard Launch", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 273, handling_address: 0x4bed20 },
    VehicleCatalogEntry { id: 278, game_name: "SKIMMER", display_name: "Skimmer Seaplane", vehicle_type: VehicleType::Boat, wheel_model_id: 0, wheel_scale: 0.00, wheel_scale_rear: 0.00, txd_id: 274, handling_address: 0x4bee00 },
    VehicleCatalogEntry { id: 279, game_name: "CHOPPER", display_name: "Police Chopper", vehicle_type: VehicleType::Helicopter, wheel_model_id: 329, wheel_scale: 0.70, wheel_scale_rear: 0.70, txd_id: 275, handling_address: 0x4beee0 },
    VehicleCatalogEntry { id: 280, game_name: "AEROPL", display_name: "Aeroplane (Twin Engine)", vehicle_type: VehicleType::PlaneOrHover, wheel_model_id: 319, wheel_scale: 1.00, wheel_scale_rear: 1.00, txd_id: 276, handling_address: 0x4befc0 },
];

impl VehicleModel {
    #[allow(dead_code)]
    pub fn load_banshee(disc: &mut Disc, game: &[u8]) -> Result<Self> {
        Self::load(disc, game, 175)
    }

    /// Decodes any vehicle model (170..=280) from GTA3PS2.IMG and GAME.DTZ.
    pub fn load(disc: &mut Disc, game: &[u8], model_id: u16) -> Result<Self> {
        let entry = VehicleCatalog::get(model_id)
            .ok_or_else(|| format!("Vehicle ID {model_id} not found in catalog"))?;
        let streaming = u32le(game, 0x7c)? as usize;
        let table = u32le(game, streaming + 0xc8)? as usize;
        let ranges = [
            u32le(game, streaming + 8)? as usize,
            u32le(game, streaming + 12)? as usize,
            u32le(game, streaming + 16)? as usize,
            u32le(game, streaming + 20)? as usize,
        ];

        let txd_slot: u16 = entry.txd_id;
        let wheel_model_id: u16 = entry.wheel_model_id;
        let wheel_scale: f32 = entry.wheel_scale;
        let paint_color = vehicle_paint_color(game, model_id)?;
        let handling_address = entry.handling_address;

        // 1. Read vehicle model data from ISO
        let model_row = table + model_id as usize * 24;
        let model_sec = u32le(game, model_row + 16)? as usize;
        let model_cnt = u32le(game, model_row + 20)? as usize;
        let model_data = disc.range("GTA3PS2.IMG", model_sec * 2048, model_cnt * 2048)?;

        // 2. Read vehicle TXD data from ISO
        let txd_stream_idx = ranges[0] + txd_slot as usize;
        let txd_row = table + txd_stream_idx * 24;
        let txd_sec = u32le(game, txd_row + 16)? as usize;
        let txd_cnt = u32le(game, txd_row + 20)? as usize;
        let txd_data = disc.range("GTA3PS2.IMG", txd_sec * 2048, txd_cnt * 2048)?;

        // 3. Load textures from vehicle TXD and embedded generic TXD
        let mut texture_sources = load_streamed_txd(&txd_data)?;
        let generic_textures = load_embedded_generic_txd(game)?;
        for (name, tex) in generic_textures {
            texture_sources.entry(name).or_insert(tex);
        }

        // 4. Decode body frames
        if bytes(&model_data, 0, 4)? != b"ldm\0" {
            return Err("Unexpected vehicle model magic".into());
        }
        let clump = u32le(&model_data, 0x20)? as usize;
        let root_frame = u32le(&model_data, clump + 4)? as usize;
        let mut frames = HashMap::new();
        walk_frames(
            &model_data,
            root_frame,
            None,
            &mut frames,
            &mut HashSet::new(),
        )?;
        let collision = crate::collision::NativeCollider::load_embedded(
            game,
            model_id,
            &frames[&root_frame].world,
        )?;

        // 5. Decode visible body atomics
        let head = clump + 8;
        let mut link = u32le(&model_data, head)? as usize;
        let mut total_atomics = 0;
        let mut visible_atomics = 0;

        let mut triangles = Vec::new();
        let mut doors = Vec::new();
        let mut assigned_textures = HashMap::new();
        let mut texture_id_alloc = VEHICLE_TEXTURE_BASE;
        let mut texture_name_to_id = HashMap::new();
        let mut paint_texture_ids = HashMap::new();

        while link != head {
            total_atomics += 1;
            let atomic = link - 0x1c;
            let flags = u32le(&model_data, atomic)?;
            let frame = u32le(&model_data, atomic + 4)? as usize;
            let geometry = u32le(&model_data, atomic + 0x14)? as usize;

            let frame_node = frames.get(&frame).ok_or("Unresolved vehicle frame")?;
            let is_visible = (flags & 0x40000) != 0 && frame_node.name != "chassis_vlo";

            if is_visible {
                visible_atomics += 1;
                let mat_count = u32le(&model_data, geometry + 16)? as usize;
                let mat_table = u32le(&model_data, geometry + 12)? as usize;
                let mut mat_colors = Vec::new();
                let mut mat_texture_ids = Vec::new();

                for i in 0..mat_count {
                    let m = u32le(&model_data, mat_table + i * 4)? as usize;
                    let tex_ptr = u32le(&model_data, m)? as usize;
                    let tex_name = if tex_ptr != 0 && tex_ptr < model_data.len() {
                        string(&model_data, tex_ptr, 32)?
                    } else {
                        String::new()
                    };
                    let rgb = bytes(&model_data, m + 4, 3)?;
                    let is_exterior_paint = matches!(
                        frame_node.name.as_str(),
                        "bonnet_dummy"
                            | "bump_front_dummy"
                            | "door_lf_dummy"
                            | "door_rf_dummy"
                            | "chassis_hi"
                            | "bump_rear_dummy"
                    ) && matches!(
                        tex_name.to_ascii_lowercase().as_str(),
                        "banshee868bit_sl" | "xv_vehiclegrunge_sl"
                    );
                    mat_colors.push(if is_exterior_paint {
                        [1.0, 1.0, 1.0]
                    } else {
                        [
                            rgb[0] as f32 / 255.0,
                            rgb[1] as f32 / 255.0,
                            rgb[2] as f32 / 255.0,
                        ]
                    });

                    let tid = if !tex_name.is_empty() {
                        let lower = tex_name.to_lowercase();
                        let original_id = if let Some(&id) = texture_name_to_id.get(&lower) {
                            id
                        } else {
                            let id = texture_id_alloc;
                            texture_id_alloc += 1;
                            texture_name_to_id.insert(lower.clone(), id);
                            if let Some(tex) = texture_sources.get(&lower) {
                                assigned_textures.insert(id, tex.clone());
                            }
                            id
                        };
                        if is_exterior_paint {
                            if let Some(&id) = paint_texture_ids.get(&lower) {
                                id
                            } else {
                                let id = texture_id_alloc;
                                texture_id_alloc += 1;
                                if let Some(tex) = texture_sources.get(&lower) {
                                    assigned_textures
                                        .insert(id, tex.painted_vehicle_surface(paint_color));
                                }
                                paint_texture_ids.insert(lower.clone(), id);
                                id
                            }
                        } else {
                            original_id
                        }
                    } else {
                        VEHICLE_TEXTURE_BASE
                    };
                    mat_texture_ids.push(tid);
                }

                let start = triangles.len();
                decode_atomic_triangles(
                    &model_data,
                    geometry,
                    frame_node.world,
                    &mat_colors,
                    &mat_texture_ids,
                    &mut triangles,
                )?;
                if matches!(frame_node.name.as_str(), "door_lf_dummy" | "door_rf_dummy") {
                    doors.push(VehicleDoor {
                        triangle_range: start..triangles.len(),
                        hinge: [
                            frame_node.world[12],
                            frame_node.world[13],
                            frame_node.world[14],
                        ],
                        side: if frame_node.name == "door_lf_dummy" {
                            -1.
                        } else {
                            1.
                        },
                    });
                }
            }
            link = u32le(&model_data, link)? as usize;
        }
        let body_triangles = triangles.len();

        // 6. Decode shared wheel model from GAME.DTZ
        let mut wheel_triangles_count = 0;
        let mut wheels = Vec::new();
        let wheel_element = 0x4c5188usize;
        if wheel_model_id != 0 && wheel_model_id != 65535 && wheel_element + 0x20 <= game.len() {
            let wheel_atomic = u32le(game, wheel_element)? as usize;
            let wheel_geom = u32le(game, wheel_atomic + 0x14)? as usize;
            let wheel_frame = u32le(game, wheel_atomic + 4)? as usize;
            let wheel_local = matrix(game, wheel_frame + 0x10)?;

            let mat_count = u32le(game, wheel_geom + 16)? as usize;
            let mat_table = u32le(game, wheel_geom + 12)? as usize;
            let mut wheel_mat_colors = Vec::new();
            let mut wheel_mat_texture_ids = Vec::new();

            for i in 0..mat_count {
                let m = u32le(game, mat_table + i * 4)? as usize;
                let t = u32le(game, m)? as usize;
                let tex_name = if t != 0 && t + 16 < game.len() {
                    string(game, t + 16, 32)?
                } else {
                    String::new()
                };
                let rgb = bytes(game, m + 4, 3)?;
                wheel_mat_colors.push([
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                ]);

                let lower = tex_name.to_lowercase();
                let tid = if let Some(&id) = texture_name_to_id.get(&lower) {
                    id
                } else {
                    let id = texture_id_alloc;
                    texture_id_alloc += 1;
                    texture_name_to_id.insert(lower.clone(), id);
                    if let Some(tex) = texture_sources.get(&lower) {
                        assigned_textures.insert(id, tex.clone());
                    }
                    id
                };
                wheel_mat_texture_ids.push(tid);
            }

            // Find all 4 wheel dummy frames in the vehicle
            for (_, frame_node) in &frames {
                if frame_node.name.starts_with("wheel_") {
                    let is_left = frame_node.name.contains("lf") || frame_node.name.contains("lb");
                    let mut scale_mat = identity();
                    scale_mat[0] = if is_left { -wheel_scale } else { wheel_scale };
                    scale_mat[5] = wheel_scale;
                    scale_mat[10] = wheel_scale;

                    let instance_world =
                        multiply(frame_node.world, multiply(scale_mat, wheel_local));
                    let mut wheel_tris = Vec::new();
                    let triangle_start = triangles.len();
                    decode_atomic_triangles(
                        game,
                        wheel_geom,
                        instance_world,
                        &wheel_mat_colors,
                        &wheel_mat_texture_ids,
                        &mut wheel_tris,
                    )?;

                    // If scale was negative on X (left wheel), reverse triangle winding so faces remain outwards
                    if is_left {
                        for tri in &mut wheel_tris {
                            tri.vertices.swap(0, 1);
                        }
                    }
                    wheel_triangles_count += wheel_tris.len();
                    let name = frame_node.name.to_ascii_lowercase();
                    wheels.push(VehicleWheel {
                        triangle_range: triangle_start..triangle_start + wheel_tris.len(),
                        center: transform(instance_world, [0.0; 3]),
                        front: name.contains("_rf") || name.contains("_lf"),
                        is_left,
                    });
                    triangles.extend(wheel_tris);
                }
            }
        }

        let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
        for tri in &triangles {
            for v in &tri.vertices {
                for j in 0..3 {
                    bounds[0][j] = bounds[0][j].min(v.p[j]);
                    bounds[1][j] = bounds[1][j].max(v.p[j]);
                }
            }
        }

        let mut resolved_texture_names: Vec<_> = texture_name_to_id
            .iter()
            .filter(|(_, id)| assigned_textures.contains_key(id))
            .map(|(name, _)| name.clone())
            .collect();
        resolved_texture_names.sort();
        let mut missing_texture_names: Vec<_> = texture_name_to_id
            .iter()
            .filter(|(_, id)| !assigned_textures.contains_key(id))
            .map(|(name, _)| name.clone())
            .collect();
        missing_texture_names.sort();
        let stats = VehicleStats {
            frame_positions: frames
                .values()
                .map(|f| (f.name.clone(), [f.world[12], f.world[13], f.world[14]]))
                .collect(),
            collision_primitives: collision.primitive_counts(),
            name: entry.game_name.to_string(),
            model_id,
            txd_id: txd_slot,
            wheel_model_id,
            wheel_scale,
            total_atomics,
            visible_atomics,
            body_triangles,
            wheel_triangles: wheel_triangles_count,
            total_triangles: triangles.len(),
            bounds,
            texture_names: texture_name_to_id.keys().cloned().collect(),
            resolved_texture_names,
            missing_texture_names,
            handling_record_address: handling_address,
            textures: texture_name_to_id
                .iter()
                .filter_map(|(name, id)| {
                    assigned_textures.get(id).map(|texture| VehicleTextureInfo {
                        name: name.clone(),
                        width: texture.width,
                        height: texture.height,
                        has_alpha: texture.has_alpha,
                    })
                })
                .collect(),
        };

        Ok(Self {
            collision,
            stats,
            triangles,
            textures: assigned_textures,
            bounds,
            wheels,
            doors,
        })
    }

    /// Rebuild only the four wheel meshes with spin and front steering for this frame.
    #[allow(dead_code)]
    pub fn animated_triangles(&self, wheel_angle: f32, steering: f32) -> Vec<Triangle> {
        self.animated_triangles_with_suspension(wheel_angle, steering, [0.0; 4])
    }

    /// Rebuild the four wheel meshes with spin, front steering, and 4-wheel independent suspension deflection.
    pub fn animated_triangles_with_suspension(
        &self,
        wheel_angle: f32,
        steering: f32,
        suspensions: [f32; 4],
    ) -> Vec<Triangle> {
        let mut triangles = self.triangles.clone();
        let (spin_s, spin_c) = wheel_angle.sin_cos();
        let (turn_s, turn_c) = steering.sin_cos();
        for wheel in &self.wheels {
            let susp_z = if wheel.front && wheel.is_left {
                suspensions[0]
            } else if wheel.front && !wheel.is_left {
                suspensions[1]
            } else if !wheel.front && wheel.is_left {
                suspensions[2]
            } else {
                suspensions[3]
            };
            for triangle in &mut triangles[wheel.triangle_range.clone()] {
                for vertex in &mut triangle.vertices {
                    let mut x = vertex.p[0] - wheel.center[0];
                    let mut y = vertex.p[1] - wheel.center[1];
                    let mut z = vertex.p[2] - wheel.center[2];
                    let spun_y = y * spin_c - z * spin_s;
                    let spun_z = y * spin_s + z * spin_c;
                    y = spun_y;
                    z = spun_z;
                    if wheel.front {
                        let turned_x = x * turn_c + y * turn_s;
                        let turned_y = -x * turn_s + y * turn_c;
                        x = turned_x;
                        y = turned_y;
                    }
                    vertex.p = [
                        x + wheel.center[0],
                        y + wheel.center[1],
                        z + wheel.center[2] + susp_z,
                    ];
                }
            }
        }
        triangles
    }

    pub fn animate_door(&self, triangles: &mut [Triangle], side: f32, opening: f32) {
        // Native dummy frames supply the hinge. The 65 degree opening is a
        // provisional controller angle, not an extracted handling value.
        let angle = opening.clamp(0., 1.) * 65f32.to_radians() * side;
        let (s, c) = angle.sin_cos();
        for door in self.doors.iter().filter(|d| d.side == side) {
            for tri in &mut triangles[door.triangle_range.clone()] {
                for v in &mut tri.vertices {
                    let x = v.p[0] - door.hinge[0];
                    let y = v.p[1] - door.hinge[1];
                    v.p[0] = door.hinge[0] + c * x - s * y;
                    v.p[1] = door.hinge[1] + s * x + c * y;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn is_watercraft(&self) -> bool {
        VehicleCatalog::get(self.stats.model_id).map_or(false, |e| e.is_watercraft())
    }

    #[allow(dead_code)]
    pub fn is_two_wheeler(&self) -> bool {
        VehicleCatalog::get(self.stats.model_id).map_or(false, |e| e.is_two_wheeler())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matrix_leaves_vector_unchanged() {
        let m = identity();
        let p = [1.5, -2.0, 3.25];
        let transformed = transform(m, p);
        for i in 0..3 {
            assert!((transformed[i] - p[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn matrix_multiplication_composes_translation() {
        let mut m1 = identity();
        m1[12] = 10.0;
        m1[13] = 5.0;
        m1[14] = -2.0;

        let mut m2 = identity();
        m2[12] = 2.0;
        m2[13] = -1.0;
        m2[14] = 4.0;

        let composed = multiply(m1, m2);
        let p = [0.0, 0.0, 0.0];
        let transformed = transform(composed, p);
        assert!((transformed[0] - 12.0).abs() < 1e-6);
        assert!((transformed[1] - 4.0).abs() < 1e-6);
        assert!((transformed[2] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn unpack_rejects_invalid_opcode() {
        let bad_data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(unpack(&bad_data, 0, 0x79000000).is_err());
    }

    #[test]
    fn vehicle_catalog_indexes_all_111_models_without_duplicates() {
        let all = VehicleCatalog::all();
        assert_eq!(all.len(), 111, "Must contain all 111 Vice City Stories vehicle entries");

        let mut ids = std::collections::HashSet::new();
        for v in all {
            assert!(v.id >= 170 && v.id <= 280, "Vehicle ID {} out of valid 170..=280 range", v.id);
            assert!(ids.insert(v.id), "Duplicate vehicle ID {}", v.id);
            assert!(!v.game_name.is_empty(), "Vehicle ID {} missing game name", v.id);
            assert!(!v.display_name.is_empty(), "Vehicle ID {} missing display name", v.id);
        }
    }

    #[test]
    fn vehicle_catalog_filters_watercraft_correctly() {
        let watercraft = VehicleCatalog::watercraft();
        assert_eq!(watercraft.len(), 17, "Expected 17 water-capable craft (boats, jetski, hovercraft, seaplane)");
        
        let names: Vec<&str> = watercraft.iter().map(|w| w.game_name).collect();
        assert!(names.contains(&"SQUALO"), "Must contain Squalo");
        assert!(names.contains(&"JETMAX"), "Must contain Jetmax");
        assert!(names.contains(&"DINGHY"), "Must contain Dinghy");
        assert!(names.contains(&"JETSKI"), "Must contain Jetski");
        assert!(names.contains(&"HOVER"), "Must contain Bovver '64 Hovercraft");
        assert!(names.contains(&"TROPIC"), "Must contain Tropic");
        assert!(names.contains(&"MARQUIS"), "Must contain Marquis");
        assert!(names.contains(&"PREDATR"), "Must contain Predator police boat");
    }

    #[test]
    fn vehicle_catalog_find_by_name_is_case_insensitive() {
        let banshee = VehicleCatalog::find("banshee").expect("Should find banshee");
        assert_eq!(banshee.id, 175);
        assert_eq!(banshee.game_name, "BANSHEE");

        let squalo = VehicleCatalog::find("SQUALO").expect("Should find SQUALO");
        assert_eq!(squalo.id, 257);
        assert_eq!(squalo.vehicle_type, VehicleType::Boat);

        let jetski = VehicleCatalog::find("Jetski").expect("Should find Jetski");
        assert_eq!(jetski.id, 233);
        assert_eq!(jetski.vehicle_type, VehicleType::Jetski);

        let infernus = VehicleCatalog::find("Infernus").expect("Should find Infernus");
        assert_eq!(infernus.id, 270);
    }

    #[test]
    fn vehicle_catalog_automobiles_and_motorcycles() {
        let autos = VehicleCatalog::automobiles();
        assert_eq!(autos.len(), 71, "Expected 71 automobiles");

        let bikes = VehicleCatalog::motorcycles();
        assert_eq!(bikes.len(), 11, "Expected 11 two-wheelers (9 motorbikes + 2 BMX)");
        let sanchez = VehicleCatalog::find("SANCHEZ").unwrap();
        assert!(sanchez.is_two_wheeler());
        assert!(!sanchez.is_watercraft());
    }


    #[test]
    #[ignore = "Requires the user's local ISO"]
    fn multi_vehicle_models_load_cleanly() {
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let test_ids = [175, 270, 257, 233, 205, 198, 264, 199];
        for id in test_ids {
            let model = VehicleModel::load(&mut disc, &game, id);
            assert!(model.is_ok(), "Failed to load vehicle ID {}: {:?}", id, model.err());
            let m = model.unwrap();
            assert!(!m.triangles.is_empty(), "Vehicle ID {} has no triangles", id);
            println!("Loaded vehicle ID {}: {} ({} triangles, {} textures)", id, m.stats.name, m.triangles.len(), m.textures.len());
        }
    }
}
