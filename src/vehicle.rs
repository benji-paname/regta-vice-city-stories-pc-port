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

fn banshee_paint_color(game: &[u8]) -> Result<[u8; 3]> {
    // ResourceImage::modelInfoPtrs and colourTable in this PS2 PAL image.
    let model_infos = u32le(game, 0x3c)? as usize;
    let model_info = u32le(game, model_infos + 175 * 4)? as usize;
    let colour_table = u32le(game, 0x90)? as usize;
    let colour_count = *bytes(game, model_info + 0x161, 1)?.first().unwrap() as usize;
    if colour_count == 0 || colour_count > 8 {
        return Err("Banshee has an invalid vehicle colour list".into());
    }
    let colours = bytes(game, model_info + 0x163, colour_count * 2)?;
    let palette = bytes(game, colour_table, 256 * 3)?;

    // Use the lightest documented Banshee palette entry, matching the white
    // car shown in the user's reference image.
    let mut selected = None;
    for pair in colours.chunks_exact(2) {
        let primary_id = pair[0] as usize;
        let primary_at = (128 + primary_id) * 3;
        let rgb = bytes(palette, primary_at, 3)?;
        let luminance = 0.2126 * rgb[0] as f32 + 0.7152 * rgb[1] as f32 + 0.0722 * rgb[2] as f32;
        if selected.is_none_or(|(_, best)| luminance > best) {
            selected = Some(([rgb[0], rgb[1], rgb[2]], luminance));
        }
    }
    selected
        .map(|(rgb, _)| rgb)
        .ok_or_else(|| "Banshee has no usable paint colour".into())
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

impl VehicleModel {
    pub fn load_banshee(disc: &mut Disc, game: &[u8]) -> Result<Self> {
        let streaming = u32le(game, 0x7c)? as usize;
        let table = u32le(game, streaming + 0xc8)? as usize;
        let ranges = [
            u32le(game, streaming + 8)? as usize,
            u32le(game, streaming + 12)? as usize,
            u32le(game, streaming + 16)? as usize,
            u32le(game, streaming + 20)? as usize,
        ];

        let model_id: u16 = 175;
        let txd_slot: u16 = 171;
        let wheel_model_id: u16 = 323;
        let wheel_scale: f32 = 0.7;
        let paint_color = banshee_paint_color(game)?;
        // Banshee's native handling record, found from the model's GAME.DTZ entry.
        let handling_address = 0x4c5700usize;

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
        let wheel_element = 0x4c5188usize; // Model 323 shared atomic element
        if wheel_element + 0x20 <= game.len() {
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
            name: "BANSHEE".to_string(),
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
}
