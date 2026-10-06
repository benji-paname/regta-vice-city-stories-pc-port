//! Original VCS PS2 player, native skin weights and animation keyframes.
//! Animation rotations are local absolute quaternions, not invented motion.
//! The controller owns locomotion: root translation remains at the bind value.

use crate::iso::{bytes, f32le, u16le, u32le, Disc, Result};
use crate::leeds::{half, Triangle, Vertex};
use crate::texture::{decode_raster, Texture};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

type Matrix = [f32; 16];
const TEXTURE_BASE: u16 = 60000;

#[derive(Serialize)]
pub struct CharacterStats {
    pub model_id: u16,
    pub texture_stream_id: u32,
    pub model_bytes: usize,
    pub texture_bytes: usize,
    pub bones: usize,
    pub source_vertices: usize,
    pub source_strip_triangles: usize,
    pub drawable_triangles: usize,
    pub textures: usize,
    pub texture_names: Vec<String>,
    pub bind_bounds: [[f32; 3]; 2],
    pub max_bind_inverse_error: f32,
    pub max_weight_sum_error: f32,
    pub clips: Vec<ClipStats>,
    pub root_translation_locked: bool,
}

#[derive(Serialize)]
pub struct ClipStats {
    pub name: String,
    pub duration_seconds: f32,
    pub rotation_tracks: usize,
    pub rotation_keys: usize,
    pub maximum_duration_error: f32,
}

#[derive(Clone, Copy, Default)]
struct Influence {
    bone: usize,
    weight: f32,
}

struct SkinVertex {
    vertex: Vertex,
    influences: [Influence; 4],
}

struct Face {
    indices: [usize; 3],
    texture_id: u16,
}

struct Bone {
    id: u8,
    parent: Option<usize>,
    bind: Matrix,
    inverse: Matrix,
}

#[derive(Clone, Copy)]
struct Key {
    time: f32,
    q: [f32; 4],
}

struct Track {
    bone: usize,
    keys: Vec<Key>,
}

struct Clip {
    name: String,
    duration: f32,
    tracks: Vec<Track>,
    max_duration_error: f32,
}

pub struct Character {
    pub stats: CharacterStats,
    pub textures: HashMap<u16, Texture>,
    vertices: Vec<SkinVertex>,
    faces: Vec<Face>,
    bones: Vec<Bone>,
    clips: Vec<Clip>,
    feet_offset: f32,
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
    // RenderWare stores flags in the fourth component of each vector.
    m[3] = 0.0;
    m[7] = 0.0;
    m[11] = 0.0;
    m[15] = 1.0;
    if m.iter().any(|x| !x.is_finite()) {
        return Err("Player matrix contains non-finite values".into());
    }
    Ok(m)
}

fn inverse_affine(m: Matrix) -> Result<Matrix> {
    let a = m[0];
    let b = m[4];
    let c = m[8];
    let d = m[1];
    let e = m[5];
    let f = m[9];
    let g = m[2];
    let h = m[6];
    let i = m[10];
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 0.000001 {
        return Err("Player matrix is singular".into());
    }
    let mut out = identity();
    out[0] = (e * i - f * h) / det;
    out[4] = (c * h - b * i) / det;
    out[8] = (b * f - c * e) / det;
    out[1] = (f * g - d * i) / det;
    out[5] = (a * i - c * g) / det;
    out[9] = (c * d - a * f) / det;
    out[2] = (d * h - e * g) / det;
    out[6] = (b * g - a * h) / det;
    out[10] = (a * e - b * d) / det;
    let translated = transform(out, [m[12], m[13], m[14]]);
    for r in 0..3 {
        out[12 + r] = -translated[r];
    }
    Ok(out)
}

fn string(data: &[u8], at: usize, max: usize) -> Result<String> {
    let tail = data.get(at..).ok_or("Player string offset is invalid")?;
    let end = tail
        .iter()
        .take(max)
        .position(|c| *c == 0)
        .ok_or("Unterminated player string")?;
    Ok(std::str::from_utf8(&tail[..end])?.to_owned())
}

fn chunk<'a>(data: &'a [u8], magic: &[u8; 4]) -> Result<&'a [u8]> {
    if bytes(data, 0, 4)? != magic {
        return Err("Unexpected player resource magic".into());
    }
    let logical = u32le(data, 8)? as usize;
    if logical < 32 {
        return Err("Player resource is too short".into());
    }
    bytes(data, 0, logical)
}

fn stream_resource(disc: &mut Disc, game: &[u8], table: usize, id: usize) -> Result<Vec<u8>> {
    let row = table
        .checked_add(id.checked_mul(24).ok_or("Streaming index overflow")?)
        .ok_or("Streaming table overflow")?;
    let sector = u32le(game, row + 16)? as usize;
    let count = u32le(game, row + 20)? as usize;
    if count == 0 || count > 4096 {
        return Err("Player resource has no valid streaming extent".into());
    }
    disc.range(
        "GTA3PS2.IMG",
        sector
            .checked_mul(2048)
            .ok_or("Streaming sector overflow")?,
        count * 2048,
    )
}

fn frame_order(
    data: &[u8],
    frame: usize,
    parent: Option<usize>,
    frames: &mut Vec<(usize, Option<usize>)>,
    seen: &mut HashSet<usize>,
) -> Result<()> {
    if frames.len() >= 64 || !seen.insert(frame) {
        return Err("Player frame hierarchy is cyclic or too large".into());
    }
    bytes(data, frame, 0xb0)?;
    let index = frames.len();
    frames.push((frame, parent));
    let mut child = u32le(data, frame + 0x90)? as usize;
    let mut children = Vec::new();
    let mut sibling_seen = HashSet::new();
    while child != 0 {
        if !sibling_seen.insert(child) || children.len() >= 64 {
            return Err("Player sibling list is cyclic".into());
        }
        children.push(child);
        child = u32le(data, child + 0x94)? as usize;
    }
    // VCS adds each child to the front of the list. The skin palette retains
    // the original hierarchy order, proven against all 25 inverse matrices.
    for child in children.into_iter().rev() {
        frame_order(data, child, Some(index), frames, seen)?;
    }
    Ok(())
}

fn unpack(data: &[u8], at: usize, expected: u32) -> Result<(usize, usize, usize)> {
    let code = u32le(data, at)?;
    if code & 0xff004000 != expected {
        return Err(format!("Unexpected player VIF opcode {code:#x} at {at:#x}").into());
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

fn require(data: &[u8], at: usize, value: u32) -> Result<()> {
    if u32le(data, at)? != value {
        return Err(format!("Unexpected player command at {at:#x}").into());
    }
    Ok(())
}

fn material_textures(data: &[u8], geometry: usize) -> Result<(Vec<String>, Vec<[f32; 3]>)> {
    let count = u32le(data, geometry + 16)? as usize;
    if count == 0 || count > 128 {
        return Err("Player material count is invalid".into());
    }
    let table = u32le(data, geometry + 12)? as usize;
    bytes(data, table, count * 4)?;
    let mut names = Vec::new();
    let mut colors = Vec::new();
    for i in 0..count {
        let material = u32le(data, table + i * 4)? as usize;
        names.push(string(data, u32le(data, material)? as usize, 128)?);
        let rgb = bytes(data, material + 4, 4)?;
        colors.push([
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
        ]);
    }
    Ok((names, colors))
}

fn load_textures(data: &[u8], names: &[String]) -> Result<HashMap<u16, Texture>> {
    let source = chunk(data, b"xet\0")?;
    let head = 0x28;
    let mut link = u32le(source, head)? as usize;
    let mut seen = HashSet::new();
    let mut found = HashMap::new();
    while link != head {
        if link < 8 || seen.len() >= 128 || !seen.insert(link) {
            return Err("Player texture dictionary is cyclic or invalid".into());
        }
        let texture = link - 8;
        let name = string(source, texture + 16, 32)?;
        let raster = u32le(source, texture)? as usize;
        bytes(source, raster, 16)?;
        let format = u16le(source, raster + 6)?;
        if u32le(source, raster)? != 0 || !matches!(format, 0x25 | 0x45) {
            return Err("Unsupported streamed player raster prefix".into());
        }
        // Streamed RslRaster and world rasters share all pixel, mip, CLUT and
        // transfer fields. Only the allocation marker and high format flags
        // differ; canonicalize those two prefix fields for the shared decoder.
        let mut canonical = source.to_vec();
        canonical[raster..raster + 4].copy_from_slice(&0xccccccccu32.to_le_bytes());
        canonical[raster + 6..raster + 8].copy_from_slice(&(format | 0xc000).to_le_bytes());
        found.insert(name, decode_raster(&canonical, raster, 0)?);
        link = u32le(source, link)? as usize;
    }
    let mut textures = HashMap::new();
    for (i, name) in names.iter().enumerate() {
        let texture = found
            .get(name)
            .ok_or_else(|| format!("Player texture {name} is missing"))?
            .clone();
        textures.insert(TEXTURE_BASE + i as u16, texture);
    }
    Ok(textures)
}

fn decode_model(
    data: &[u8],
) -> Result<(
    Vec<SkinVertex>,
    Vec<Face>,
    Vec<Bone>,
    Vec<String>,
    Matrix,
    usize,
)> {
    let data = chunk(data, b"ldm\0")?;
    let clump = u32le(data, 0x24)? as usize;
    require(data, clump, 0x0000aa02)?;
    let list = clump + 8;
    let link = u32le(data, list)? as usize;
    let atomic = link.checked_sub(0x1c).ok_or("Invalid player atomic link")?;
    if u32le(data, link)? as usize != list {
        return Err("Player model has an unsupported number of atomics".into());
    }
    require(data, atomic, 0x0004aa01)?;
    let atomic_frame = u32le(data, atomic + 4)? as usize;
    let atomic_matrix = matrix(data, atomic_frame + 0x10)?;
    let geometry = u32le(data, atomic + 0x14)? as usize;
    let skin = u32le(data, geometry + 0x18)? as usize;
    let num_bones = u32le(data, skin)? as usize;
    let inv_matrix = u32le(data, skin + 12)? as usize;
    let hierarchy = u32le(data, atomic + 0x2c)? as usize;
    require(data, hierarchy, 0x3000)?;
    if num_bones != 25 || u32le(data, hierarchy + 4)? as usize != num_bones {
        return Err("Expected the original 25-bone VCS player".into());
    }
    let node_info = u32le(data, hierarchy + 0x30)? as usize;
    bytes(data, node_info, num_bones * 8)?;
    let anchor = u32le(data, hierarchy + 0x34)? as usize;
    let mut frames = Vec::new();
    frame_order(data, anchor, None, &mut frames, &mut HashSet::new())?;
    if frames.len() != num_bones {
        return Err("Player hierarchy does not match its skin palette".into());
    }
    let mut bones = Vec::new();
    for (i, (frame, parent)) in frames.into_iter().enumerate() {
        let row = bytes(data, node_info + i * 8, 8)?;
        if row[1] as usize != i {
            return Err("Player skin palette is not in native hierarchy order".into());
        }
        bones.push(Bone {
            id: row[0],
            parent,
            bind: matrix(data, frame + 0x10)?,
            inverse: matrix(data, inv_matrix + i * 64)?,
        });
    }
    let (names, colors) = material_textures(data, geometry)?;
    let header = geometry + 0x20;
    let strips = (u32le(data, header + 16)? >> 20) as usize;
    let flags = u32le(data, header + 20)?;
    if strips == 0 || strips > 512 || flags & 0x17 != 0x17 || flags & !0x1f != 0 {
        return Err("Unsupported player native geometry flags".into());
    }
    let scale = [
        f32le(data, header + 40)?,
        f32le(data, header + 44)?,
        f32le(data, header + 48)?,
    ];
    let origin = [
        f32le(data, header + 52)?,
        f32le(data, header + 56)?,
        f32le(data, header + 60)?,
    ];
    if scale.iter().chain(origin.iter()).any(|x| !x.is_finite()) {
        return Err("Invalid player geometry transform".into());
    }
    let strip_table = header + 64;
    bytes(data, strip_table, strips * 48)?;
    let dma_base = strip_table + strips * 48;
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut source_triangles = 0;
    for si in 0..strips {
        let row = strip_table + si * 48;
        let uv_scale = [f32le(data, row + 16)?, f32le(data, row + 20)?];
        let triangle_count = u16le(data, row + 32)? as usize;
        let material = u16le(data, row + 34)? as usize;
        if material >= colors.len() || uv_scale.iter().any(|x| !x.is_finite()) {
            return Err("Invalid player material or UV scale".into());
        }
        let packet = dma_base + u32le(data, row + 28)? as usize;
        let length = ((u32le(data, packet)? & 0xffff) as usize + 1) * 16;
        bytes(data, packet, length)?;
        let end = packet + length;
        let start_vertex = vertices.len();
        let mut cursor = packet + 16;
        let mut batch = 0;
        while cursor < end {
            require(data, cursor, 0x6c018000)?;
            let count = (u32le(data, cursor + 16)? & 0x7fff) as usize;
            cursor += 20;
            require(data, cursor, 0x20000000)?;
            cursor += 8;
            require(data, cursor, 0x30000000)?;
            cursor += 20;
            let (positions, next, nv) = unpack(data, cursor, 0x79000000)?;
            cursor = next;
            require(data, cursor, 0x20000000)?;
            cursor += 8;
            require(data, cursor, 0x30000000)?;
            cursor += 20;
            let (uvs, next, nt) = unpack(data, cursor, 0x76004000)?;
            cursor = next;
            let mut colors_at = None;
            if flags & 8 != 0 {
                let (p, next, n) = unpack(data, cursor, 0x6f000000)?;
                if n != count {
                    return Err("Player prelight count is invalid".into());
                }
                colors_at = Some(p);
                cursor = next;
            }
            let (_, next, nn) = unpack(data, cursor, 0x6a000000)?;
            cursor = next;
            let (weights, next, nw) = unpack(data, cursor, 0x6c000000)?;
            cursor = next;
            if [nv, nt, nn, nw].iter().any(|n| *n != count) || count < 3 || cursor >= end {
                return Err("Player VIF vertex streams disagree".into());
            }
            require(data, cursor, 0x14000006)?;
            cursor += 4;
            while cursor < end && u32le(data, cursor)? == 0 {
                cursor += 4;
            }
            if cursor > end {
                return Err("Player VIF packet exceeded its declared length".into());
            }
            for v in if batch == 0 { 0 } else { 2 }..count {
                let mut position = [0.0; 3];
                for j in 0..3 {
                    let raw = u16le(data, positions + v * 6 + j * 2)? as i16;
                    position[j] = raw as f32 / 32767.5 * scale[j] + origin[j];
                }
                let mut influences = [Influence::default(); 4];
                let mut sum = 0.0;
                for j in 0..4 {
                    let word = u32le(data, weights + v * 16 + j * 4)?;
                    let weight = f32::from_bits(word & 0xffffff00);
                    let bone = ((word >> 2) & 0x3f) as usize;
                    if !weight.is_finite()
                        || !(0.0..=1.0).contains(&weight)
                        || (weight > 0.0 && bone >= bones.len())
                    {
                        return Err("Invalid native player skin influence".into());
                    }
                    influences[j] = Influence {
                        bone: if weight == 0.0 { 0 } else { bone },
                        weight,
                    };
                    sum += weight;
                }
                if (sum - 1.0).abs() > 0.001 {
                    return Err("Player skin weights do not sum to one".into());
                }
                let mut color = colors[material];
                if let Some(p) = colors_at {
                    let prelight = u16le(data, p + v * 2)?;
                    for j in 0..3 {
                        color[j] *= ((prelight >> (j * 5)) & 31) as f32 / 31.0;
                    }
                }
                vertices.push(SkinVertex {
                    vertex: Vertex {
                        p: position,
                        color,
                        uv: [
                            data[uvs + v * 2] as f32 / 127.5 * uv_scale[0],
                            data[uvs + v * 2 + 1] as f32 / 127.5 * uv_scale[1],
                        ],
                    },
                    influences,
                });
            }
            batch += 1;
        }
        if vertices.len() - start_vertex != triangle_count + 2 {
            return Err("Player strip triangle count does not match VIF data".into());
        }
        source_triangles += triangle_count;
        for k in 2..triangle_count + 2 {
            let indices = if k & 1 == 0 {
                [start_vertex + k - 2, start_vertex + k - 1, start_vertex + k]
            } else {
                [start_vertex + k - 1, start_vertex + k - 2, start_vertex + k]
            };
            let a = vertices[indices[0]].vertex.p;
            let b = vertices[indices[1]].vertex.p;
            let c = vertices[indices[2]].vertex.p;
            if a == b || b == c || a == c {
                continue;
            }
            faces.push(Face {
                indices,
                texture_id: TEXTURE_BASE + material as u16,
            });
        }
    }
    if vertices.len() != u16le(data, header + 24)? as usize {
        return Err("Player total vertex count disagrees with its native header".into());
    }
    Ok((
        vertices,
        faces,
        bones,
        names,
        atomic_matrix,
        source_triangles,
    ))
}

fn normalize(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length > 0.000001 && length.is_finite() {
        q.map(|x| x / length)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

fn interpolate(a: [f32; 4], mut b: [f32; 4], factor: f32) -> [f32; 4] {
    let mut dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    if dot < 0.0 {
        b = b.map(|x| -x);
        dot = -dot;
    }
    let t = factor.clamp(0.0, 1.0);
    if dot > 0.9995 {
        return normalize(std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t));
    }
    let angle = dot.clamp(-1.0, 1.0).acos();
    let denom = angle.sin();
    let left = ((1.0 - t) * angle).sin() / denom;
    let right = (t * angle).sin() / denom;
    normalize(std::array::from_fn(|i| a[i] * left + b[i] * right))
}

fn rotation(q: [f32; 4], mut m: Matrix) -> Matrix {
    let [x, y, z, w] = normalize(q);
    m[0] = 1.0 - 2.0 * y * y - 2.0 * z * z;
    m[4] = 2.0 * x * y - 2.0 * z * w;
    m[8] = 2.0 * x * z + 2.0 * y * w;
    m[1] = 2.0 * x * y + 2.0 * z * w;
    m[5] = 1.0 - 2.0 * x * x - 2.0 * z * z;
    m[9] = 2.0 * y * z - 2.0 * x * w;
    m[2] = 2.0 * x * z - 2.0 * y * w;
    m[6] = 2.0 * y * z + 2.0 * x * w;
    m[10] = 1.0 - 2.0 * x * x - 2.0 * y * y;
    m
}

fn load_clips(game: &[u8], bones: &[Bone]) -> Result<Vec<Clip>> {
    let manager = u32le(game, 0x80)? as usize;
    let tree_table = u32le(game, manager + 0x1013c)? as usize;
    let count = u32le(game, manager + 0x10144)? as usize;
    if count == 0 || count > 4096 {
        return Err("Invalid VCS animation manager".into());
    }
    bytes(game, tree_table, count * 40)?;
    let wanted = ["IDLE_stance", "WALK_player", "run_player"];
    let mut clips: Vec<Clip> = Vec::new();
    for i in 0..count {
        let tree = tree_table + i * 40;
        let name = string(game, tree + 4, 24)?;
        if !wanted.contains(&name.as_str()) {
            continue;
        }
        // Later unloaded banks can contain another entry with the same name.
        if clips.iter().any(|clip| clip.name == name) {
            continue;
        }
        let channels = u32le(game, tree)? as usize;
        let count = u16le(game, tree + 28)? as usize;
        let duration = f32le(game, tree + 32)?;
        if channels == 0 {
            continue;
        }
        if count == 0 || count > 64 || !duration.is_finite() || !(0.01..120.0).contains(&duration) {
            return Err(format!("Animation {name} is not loaded safely").into());
        }
        bytes(game, channels, count * 12)?;
        let mut tracks = Vec::new();
        let mut max_duration_error: f32 = 0.0;
        for channel in 0..count {
            let descriptor = channels + channel * 12;
            let flags = u16le(game, descriptor)?;
            let frames = u16le(game, descriptor + 2)? as usize;
            let key_offset = u32le(game, descriptor + 4)? as usize;
            let tag = u32le(game, descriptor + 8)?;
            if frames == 0 {
                continue;
            }
            if frames > 4096 || flags & !0x1f != 0 || flags & 1 == 0 || flags & 4 != 0 {
                return Err("Unsupported player animation channel format".into());
            }
            if tag & 0xffff > 255 {
                return Err("Player animation bone id is not a direct native id".into());
            }
            let id = (tag & 0xffff) as u8;
            let bone = bones
                .iter()
                .position(|b| b.id == id && id != 255)
                .ok_or_else(|| format!("Animation {name} has an unresolved bone tag {tag:#x}"))?;
            if flags & 2 != 0 && bone != 0 {
                return Err("Non-root player translation channels are not implemented".into());
            }
            let stride = if flags & 2 != 0 { 16 } else { 10 };
            bytes(game, key_offset, frames * stride)?;
            let mut keys = Vec::new();
            let mut time = 0.0;
            for frame in 0..frames {
                let at = key_offset + frame * stride;
                let q = [
                    half(u16le(game, at)?),
                    half(u16le(game, at + 2)?),
                    half(u16le(game, at + 4)?),
                    half(u16le(game, at + 6)?),
                ];
                let delta = half(u16le(game, at + 8)?);
                let length = q.iter().map(|x| x * x).sum::<f32>().sqrt();
                if q.iter().any(|x| !x.is_finite())
                    || !(0.9..1.1).contains(&length)
                    || !delta.is_finite()
                    || !(0.0..120.0).contains(&delta)
                {
                    return Err("Player animation key contains invalid values".into());
                }
                if flags & 2 != 0 {
                    for j in 0..3 {
                        if !half(u16le(game, at + 10 + j * 2)?).is_finite() {
                            return Err("Player root motion is invalid".into());
                        }
                    }
                }
                time += delta;
                keys.push(Key {
                    time,
                    q: normalize(q),
                });
            }
            // A single zero-time key is a constant pose held for the clip.
            if keys.len() > 1 {
                max_duration_error = max_duration_error.max((time - duration).abs());
            }
            tracks.push(Track { bone, keys });
        }
        if tracks.is_empty() || max_duration_error > 0.01 {
            return Err("Player animation intervals disagree with clip duration".into());
        }
        clips.push(Clip {
            name,
            duration,
            tracks,
            max_duration_error,
        });
    }
    if clips.len() != wanted.len() {
        return Err("A required original player animation is missing".into());
    }
    Ok(clips)
}

impl Character {
    pub fn load(disc: &mut Disc, game: &[u8]) -> Result<Self> {
        let streaming = u32le(game, 0x7c)? as usize;
        let table = u32le(game, streaming + 0xc8)? as usize;
        let model_count = u32le(game, 0x38)? as usize;
        if model_count == 0 {
            return Err("Game has no player model entry".into());
        }
        let info_table = u32le(game, 0x3c)? as usize;
        let player_info = u32le(game, info_table)? as usize;
        if bytes(game, player_info + 16, 1)?[0] != 7 {
            return Err("Player model is not a VCS pedestrian".into());
        }
        let texture_slot = u16le(game, player_info + 0x1e)? as usize;
        let texture_id = (u32le(game, streaming + 8)? as usize)
            .checked_add(texture_slot)
            .ok_or("Player texture index overflow")?;
        let model_data = stream_resource(disc, game, table, 0)?;
        let texture_data = stream_resource(disc, game, table, texture_id)?;
        let (vertices, faces, bones, names, atomic, source_triangles) = decode_model(&model_data)?;
        let textures = load_textures(&texture_data, &names)?;
        let clips = load_clips(game, &bones)?;
        let mut worlds = Vec::new();
        let inverse_atomic = inverse_affine(atomic)?;
        let mut bind_error: f32 = 0.0;
        for bone in &bones {
            let world = multiply(
                bone.parent.map(|p| worlds[p]).unwrap_or_else(identity),
                bone.bind,
            );
            let check = multiply(multiply(inverse_atomic, world), bone.inverse);
            for i in 0..16 {
                bind_error = bind_error.max((check[i] - identity()[i]).abs());
            }
            worlds.push(world);
        }
        if bind_error > 0.0001 {
            return Err("Player bind frames and inverse matrices disagree".into());
        }
        let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
        let mut weight_error: f32 = 0.0;
        for v in &vertices {
            let p = transform(atomic, v.vertex.p);
            for j in 0..3 {
                bounds[0][j] = bounds[0][j].min(p[j]);
                bounds[1][j] = bounds[1][j].max(p[j]);
            }
            weight_error =
                weight_error.max((v.influences.iter().map(|s| s.weight).sum::<f32>() - 1.0).abs());
        }
        let stats = CharacterStats {
            model_id: 0,
            texture_stream_id: texture_id as u32,
            model_bytes: u32le(&model_data, 8)? as usize,
            texture_bytes: u32le(&texture_data, 8)? as usize,
            bones: bones.len(),
            source_vertices: vertices.len(),
            source_strip_triangles: source_triangles,
            drawable_triangles: faces.len(),
            textures: textures.len(),
            texture_names: names,
            bind_bounds: bounds,
            max_bind_inverse_error: bind_error,
            max_weight_sum_error: weight_error,
            clips: clips
                .iter()
                .map(|c| ClipStats {
                    name: c.name.clone(),
                    duration_seconds: c.duration,
                    rotation_tracks: c.tracks.len(),
                    rotation_keys: c.tracks.iter().map(|t| t.keys.len()).sum(),
                    maximum_duration_error: c.max_duration_error,
                })
                .collect(),
            root_translation_locked: true,
        };
        Ok(Self {
            stats,
            textures,
            vertices,
            faces,
            bones,
            clips,
            feet_offset: -bounds[0][2],
        })
    }

    /// Original skin matrices; unknown clip names deliberately select bind pose.
    /// The returned model faces +Y, with its bind feet at Z=0. Locomotion root
    /// translation is locked to prevent animation movement bypassing collisions.
    pub fn triangles(&self, clip: &str, seconds: f32) -> Vec<Triangle> {
        let requested = match clip {
            "idle" => "IDLE_stance",
            "walk" => "WALK_player",
            "run" => "run_player",
            _ => clip,
        };
        let selected = self
            .clips
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(requested));
        let mut locals: Vec<Matrix> = self.bones.iter().map(|b| b.bind).collect();
        if let Some(clip) = selected {
            let time = if seconds.is_finite() {
                seconds.rem_euclid(clip.duration)
            } else {
                0.0
            };
            for track in &clip.tracks {
                let next = track.keys.partition_point(|k| k.time <= time);
                let q = if next == 0 {
                    track.keys[0].q
                } else if next >= track.keys.len() {
                    track.keys[track.keys.len() - 1].q
                } else {
                    let a = track.keys[next - 1];
                    let b = track.keys[next];
                    let interval = b.time - a.time;
                    interpolate(
                        a.q,
                        b.q,
                        if interval > 0.0 {
                            (time - a.time) / interval
                        } else {
                            1.0
                        },
                    )
                };
                locals[track.bone] = rotation(q, locals[track.bone]);
            }
        }
        let mut worlds = Vec::with_capacity(self.bones.len());
        let mut palette = Vec::with_capacity(self.bones.len());
        for (i, bone) in self.bones.iter().enumerate() {
            let world = multiply(
                bone.parent.map(|p| worlds[p]).unwrap_or_else(identity),
                locals[i],
            );
            worlds.push(world);
            palette.push(multiply(world, bone.inverse));
        }
        let posed: Vec<Vertex> = self
            .vertices
            .iter()
            .map(|v| {
                let mut p = [0.0; 3];
                for influence in v.influences {
                    if influence.weight == 0.0 {
                        continue;
                    }
                    let value = transform(palette[influence.bone], v.vertex.p);
                    for j in 0..3 {
                        p[j] += value[j] * influence.weight;
                    }
                }
                p[2] += self.feet_offset;
                Vertex { p, ..v.vertex }
            })
            .collect();
        self.faces
            .iter()
            .map(|f| Triangle {
                vertices: f.indices.map(|i| posed[i]),
                texture_id: f.texture_id,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slerp_uses_short_arc_and_keeps_unit_quaternion() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, -1.0, 0.0];
        let q = interpolate(a, b, 0.5);
        assert!((q.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 0.00001);
        assert!((q[2].abs() - 0.5f32.sqrt()).abs() < 0.00001);
        assert_eq!(interpolate(a, a.map(|x| -x), 0.5), a);
    }

    #[test]
    fn affine_inverse_undoes_rotation_translation_and_scale() {
        let mut m = rotation([0.0, 0.0, 0.5, 0.5], identity());
        for i in 0..3 {
            m[i] *= 2.0;
            m[4 + i] *= 3.0;
            m[8 + i] *= 4.0;
        }
        m[12] = 1.0;
        m[13] = -4.0;
        m[14] = 8.0;
        let inv = inverse_affine(m).unwrap();
        let id = multiply(m, inv);
        for i in 0..16 {
            assert!((id[i] - identity()[i]).abs() < 0.00001);
        }
    }

    #[test]
    fn malformed_native_model_does_not_decode() {
        assert!(decode_model(&[]).is_err());
        let mut data = vec![0; 128];
        data[..4].copy_from_slice(b"ldm\0");
        data[8..12].copy_from_slice(&128u32.to_le_bytes());
        data[0x24..0x28].copy_from_slice(&0xfffffff0u32.to_le_bytes());
        assert!(decode_model(&data).is_err());
    }
}
