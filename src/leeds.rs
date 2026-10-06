use crate::iso::{bytes, f32le, u16le, u32le, Disc, Result};
use crate::texture::{decode_raster, Texture};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;

pub fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    const MAX: u64 = 128 * 1024 * 1024;
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder.by_ref().take(MAX + 1).read_to_end(&mut out)?;
    if out.len() as u64 > MAX {
        return Err("Inflated data exceeds 128 MiB".into());
    }
    Ok(out)
}
pub fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 31;
    let frac = (h & 1023) as f32;
    match exp {
        0 => sign * frac * 2.0f32.powi(-24),
        31 => {
            if frac == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + frac / 1024.0) * 2.0f32.powi(exp as i32 - 15),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub p: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
}
#[derive(Clone, Copy)]
pub struct Triangle {
    pub vertices: [Vertex; 3],
    pub texture_id: u16,
}
#[derive(Clone)]
pub struct Mesh {
    pub triangles: Vec<Triangle>,
}
#[derive(Clone, Serialize)]
pub struct Instance {
    pub id: u16,
    pub resource: u16,
    pub pass: usize,
    pub matrix: [f32; 16],
    pub sphere: [f32; 4],
}
#[derive(Serialize)]
pub struct LevelStats {
    pub name: String,
    pub inflated_bytes: usize,
    pub resources: usize,
    pub sectors: usize,
    pub instances: usize,
    pub loaded_meshes: usize,
    pub mesh_triangles: usize,
    pub drawable_instances: usize,
    pub unsupported_meshes: usize,
    pub unresolved_instances: usize,
    pub loaded_textures: usize,
    pub textured_mesh_triangles: usize,
    pub loaded_swap_geometries: usize,
    pub loaded_swap_textures: usize,
    pub alpha_textures: usize,
    pub indexed4_textures: usize,
    pub indexed8_textures: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Visibility {
    off: u8,
    on: u8,
}
impl Visibility {
    fn allows(self, hour: u8) -> bool {
        if self.off == 255 {
            return false;
        }
        if self.off & 128 == 0 {
            return self.on == 0;
        }
        let end = self.off & 127;
        if self.on <= end {
            hour >= self.on && hour < end
        } else {
            hour >= self.on || hour < end
        }
    }
}
pub struct Level {
    pub stats: LevelStats,
    pub instances: Vec<Instance>,
    pub meshes: HashMap<u16, Mesh>,
    pub textures: HashMap<u16, Texture>,
    resource_visibility: HashMap<u16, Vec<Visibility>>,
    instance_visibility: HashMap<u16, Visibility>,
}
impl Level {
    pub fn visible(&self, instance: &Instance, hour: u8) -> bool {
        self.resource_visibility
            .get(&instance.resource)
            .map_or(true, |conditions| conditions.iter().any(|v| v.allows(hour)))
            && self
                .instance_visibility
                .get(&instance.id)
                .map_or(true, |v| v.allows(hour))
    }
}

fn unpack_end(data: &[u8], off: usize) -> Result<usize> {
    let command = u32le(data, off)?;
    let count = ((command >> 16) & 255) as usize;
    let count = if count == 0 { 256 } else { count };
    let stride = if command & 0x6f000000 == 0x6f000000 {
        2
    } else {
        (((command >> 26) & 3) + 1) as usize * [32, 16, 8, 16][((command >> 24) & 3) as usize] / 8
    };
    let end = off + 4 + (count * stride + 3) / 4 * 4;
    bytes(data, off, end - off)?;
    Ok(end)
}

// PS2 world geometry uses VIF batches of quantized positions, UVs and RGB555.
// The three batch streams are decoded independently, with no EE/VU execution.
fn decode_packet(data: &[u8], uv_scale: [f32; 2]) -> Result<Vec<Vertex>> {
    let mut vertices = Vec::new();
    let mut off = 0;
    let mut batch = 0;
    while off < data.len() {
        while off + 4 <= data.len() && u32le(data, off)? == 0 {
            off += 4;
        }
        if off == data.len() {
            break;
        }
        if u32le(data, off)? != 0x6c018000 {
            return Err("Unsupported VIF packet start".into());
        }
        let count = (u32le(data, off + 16)? & 0x7fff) as usize;
        if count < 3 || count > 256 {
            return Err("Invalid VIF vertex count".into());
        }
        off += 20;
        if u32le(data, off)? != 0x20000000 || u32le(data, off + 8)? != 0x30000000 {
            return Err("Unsupported position mask".into());
        }
        off += 28;
        if u32le(data, off)? & 0xff004000 != 0x79000000 {
            return Err("Unsupported position encoding".into());
        }
        let positions = off + 4;
        off = unpack_end(data, off)?;
        if u32le(data, off)? != 0x20000000 || u32le(data, off + 8)? != 0x30000000 {
            return Err("Unsupported UV mask".into());
        }
        off += 28;
        if u32le(data, off)? & 0xff004000 != 0x76004000 {
            return Err("Unsupported UV encoding".into());
        }
        let coordinates = off + 4;
        off = unpack_end(data, off)?;
        if u32le(data, off)? & 0xff004000 != 0x6f000000 {
            return Err("Unsupported color encoding".into());
        }
        let colors = off + 4;
        off = unpack_end(data, off)?;
        if u32le(data, off)? != 0x14000006 {
            return Err("Unsupported VU call".into());
        }
        off += 4;
        for i in if batch == 0 { 0 } else { 2 }..count {
            let mut p = [0.0; 3];
            for (axis, value) in p.iter_mut().enumerate() {
                *value = (u16le(data, positions + i * 6 + axis * 2)? as i16) as f32 / 32767.0;
            }
            let c = u16le(data, colors + i * 2)?;
            let color = [
                (c & 31) as f32 / 31.0,
                ((c >> 5) & 31) as f32 / 31.0,
                ((c >> 10) & 31) as f32 / 31.0,
            ];
            let uv = [
                bytes(data, coordinates + i * 2, 1)?[0] as f32 / 128.0 * uv_scale[0],
                bytes(data, coordinates + i * 2 + 1, 1)?[0] as f32 / 128.0 * uv_scale[1],
            ];
            vertices.push(Vertex { p, color, uv });
        }
        batch += 1;
    }
    Ok(vertices)
}

fn geometry(data: &[u8], start: usize) -> Result<Mesh> {
    let count = u16le(data, start)? as usize;
    let descriptors = u16le(data, start + 2)? as usize;
    if count == 0 || count > 512 || descriptors < count * 24 || descriptors > count * 24 + 32 {
        return Err("Not a supported world geometry".into());
    }
    bytes(data, start + 4, descriptors)?;
    let mut packet = start + 4 + descriptors;
    let mut triangles = Vec::new();
    for index in 0..count {
        let row = start + 4 + index * 24;
        let texture_id = u16le(data, row + 4)?;
        let uv_scale = [half(u16le(data, row + 6)?), half(u16le(data, row + 8)?)];
        if !uv_scale.iter().all(|v| v.is_finite()) {
            return Err("Invalid texture UV scale".into());
        }
        let size = (u32le(data, row)? >> 1) as usize;
        if size == 0 || size > 4 * 1024 * 1024 {
            return Err("Invalid world packet size".into());
        }
        let vertices = decode_packet(bytes(data, packet, size)?, uv_scale)?;
        packet += size;
        for i in 2..vertices.len() {
            let tri = if i & 1 == 0 {
                [vertices[i - 2], vertices[i - 1], vertices[i]]
            } else {
                [vertices[i - 1], vertices[i - 2], vertices[i]]
            };
            let a = tri[0].p;
            let b = tri[1].p;
            let c = tri[2].p;
            let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let f = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let normal = [
                e[1] * f[2] - e[2] * f[1],
                e[2] * f[0] - e[0] * f[2],
                e[0] * f[1] - e[1] * f[0],
            ];
            if normal.iter().map(|v| v * v).sum::<f32>() > 1e-14 {
                triangles.push(Triangle {
                    vertices: tri,
                    texture_id,
                });
            }
        }
    }
    Ok(Mesh { triangles })
}

pub fn load_level(disc: &mut Disc, name: &str) -> Result<Level> {
    let lv = inflate(&disc.read(&format!("{name}.LVZ"))?)?;
    if bytes(&lv, 0, 4)? != b"DLRW" || u32le(&lv, 8)? as usize != lv.len() {
        return Err("Invalid master WRLD chunk".into());
    }
    let resources = u32le(&lv, 0x14c)? as usize;
    if resources > 65536 {
        return Err("Invalid resource count".into());
    }
    let table = u32le(&lv, 0x20)? as usize;
    bytes(&lv, table, resources * 12)?;
    let mut meshes = HashMap::new();
    let mut textures = HashMap::new();
    let mut attempted = HashSet::new();
    let mut unsupported = 0;
    for i in 0..resources {
        let ptr = u32le(&lv, table + i * 12)? as usize;
        if ptr >= 0x40 && ptr < lv.len() {
            if let Ok(mesh) = geometry(&lv, ptr) {
                meshes.insert(i as u16, mesh);
            } else if let Ok(texture) = decode_raster(&lv, ptr, 0) {
                textures.insert(i as u16, texture);
            }
        }
    }
    let mut instances = Vec::new();
    let mut seen = HashSet::new();
    let mut sectors = 0;
    let mut swap_headers = BTreeMap::new();
    let imgname = format!("{name}.IMG");
    for y in 0..36 {
        let first = u32le(&lv, 0x24 + y * 8)? as usize;
        let next = u32le(&lv, 0x24 + (y + 1) * 8)? as usize;
        let first_x = u32le(&lv, 0x28 + y * 8)? as usize;
        if next < first || (next - first) % 32 != 0 || (next - first) / 32 > 32 {
            return Err("Invalid sector row".into());
        }
        for col in 0..(next - first) / 32 {
            let header = first + col * 32;
            if bytes(&lv, header, 4)? != b"DLRW" {
                return Err("Invalid sector identifier".into());
            }
            let file_size = u32le(&lv, header + 8)? as usize;
            let img_offset = u32le(&lv, header + 24)? as usize;
            if file_size < 32 {
                return Err("Invalid sector size".into());
            }
            let data = disc.range(&imgname, img_offset, file_size - 32)?;
            sectors += 1;
            let swap_count = u16le(&data, 44)? as usize;
            let swap_pointer = u32le(&data, 48)? as usize;
            if swap_count > 0 {
                if swap_pointer < 32 {
                    return Err("Invalid sector swap pointer".into());
                }
                for swap in 0..swap_count {
                    let row = swap_pointer - 32 + swap * 4;
                    let condition = Visibility {
                        off: bytes(&data, row, 1)?[0],
                        on: bytes(&data, row + 1, 1)?[0],
                    };
                    let destination = u16le(&data, row + 2)?;
                    swap_headers.insert(destination, condition);
                }
            }
            let origin = [
                -2400.0 + 62.5 + 125.0 * (first_x + col) as f32 - (y & 1) as f32 * 62.5,
                -2000.0 + 54.125 + 108.25 * y as f32,
                0.0,
            ];
            let num = u16le(&data, 4)? as usize;
            let overlay = u32le(&data, 0)? as usize;
            if overlay >= 32 {
                for i in 0..num {
                    let row = overlay - 32 + i * 8;
                    let id = u32le(&data, row)?;
                    let ptr = u32le(&data, row + 4)? as usize;
                    if id < resources as u32 && ptr >= 32 {
                        if let Ok(mesh) = geometry(&data, ptr - 32) {
                            meshes.insert(id as u16, mesh);
                        } else if let Ok(texture) = decode_raster(&data, ptr - 32, -32) {
                            textures.insert(id as u16, texture);
                        }
                    }
                }
            }
            let passes: Vec<usize> = (0..9)
                .map(|i| u32le(&data, 8 + i * 4).map(|p| p as usize))
                .collect::<Result<_>>()?;
            for pass in 0..8 {
                let start = passes[pass];
                let end = passes[pass + 1];
                if start < 32 || end < start || end - 32 > data.len() {
                    return Err("Invalid instance span".into());
                }
                for row in (start - 32..end.saturating_sub(32 + 79)).step_by(80) {
                    let id = u16le(&data, row)?;
                    let resource = u16le(&data, row + 2)?;
                    if resource as usize >= resources {
                        return Err("Instance resource out of range".into());
                    }
                    let mut matrix = [0.0; 16];
                    for (i, value) in matrix.iter_mut().enumerate() {
                        *value = f32le(&data, row + 16 + i * 4)?;
                    }
                    // Matrix padding stores runtime flags, so only XYZ components are floats.
                    if (0..4).any(|c| (0..3).any(|r| !matrix[c * 4 + r].is_finite())) {
                        return Err("Nonfinite instance matrix".into());
                    }
                    for i in 0..3 {
                        matrix[12 + i] += origin[i];
                    }
                    let sphere = [
                        half(u16le(&data, row + 4)?),
                        half(u16le(&data, row + 6)?),
                        half(u16le(&data, row + 8)?),
                        half(u16le(&data, row + 10)?),
                    ];
                    let mut key = vec![(id & 0x7fff) as u32, resource as u32, pass as u32];
                    key.extend(sphere.iter().map(|v| v.to_bits()));
                    for c in 0..3 {
                        for r in 0..3 {
                            key.push(matrix[c * 4 + r].to_bits());
                        }
                    }
                    if seen.insert(key) {
                        instances.push(Instance {
                            id: id & 0x7fff,
                            resource,
                            pass,
                            matrix,
                            sphere,
                        });
                    }
                }
            }
        }
    }
    // AREA contains additional resource rows with chunk-relative pointers.
    let areas = u32le(&lv, 0x2f0)? as usize;
    let area_table = u32le(&lv, 0x2f4)? as usize;
    if areas > 1024 {
        return Err("Invalid AREA count".into());
    }
    for i in 0..areas {
        let row = area_table + i * 16;
        let off = u32le(&lv, row + 4)? as usize;
        let size = u32le(&lv, row + 8)? as usize;
        let data = disc.range(&imgname, off, size)?;
        if bytes(&data, 0, 4)? != b"AREA" && bytes(&data, 0, 4)? != b"AERA" {
            return Err("Invalid AREA chunk".into());
        }
        let num = u32le(&data, 32)? as usize;
        let table = u32le(&data, 36)? as usize;
        if num > resources {
            return Err("Invalid AREA resource count".into());
        }
        for r in 0..num {
            let row = table + r * 8;
            let id = u16le(&data, row)?;
            let ptr = u32le(&data, row + 4)? as usize;
            if id as usize >= resources {
                continue;
            }
            if let Ok(mesh) = geometry(&data, ptr) {
                meshes.insert(id, mesh);
            } else if let Ok(texture) = decode_raster(&data, ptr, 0) {
                textures.insert(id, texture);
            }
        }
    }
    // Overlay-only WRLD chunks carry timed/state resources. They are not normal
    // sectors and must never be interpreted as new instance pass arrays.
    let mut needed: HashSet<u16> = instances
        .iter()
        .filter(|i| !meshes.contains_key(&i.resource))
        .map(|i| i.resource)
        .collect();
    let mut resource_visibility: HashMap<u16, Vec<Visibility>> = HashMap::new();
    let mut loaded_swap_geometries = 0;
    let mut loaded_swap_textures = 0;
    let first_header = u32le(&lv, 0x24)? as usize;
    for (destination, condition) in swap_headers {
        let header = first_header + destination as usize * 32;
        if bytes(&lv, header, 4)? != b"DLRW" {
            return Err("Invalid swap WRLD header".into());
        }
        let size = u32le(&lv, header + 8)? as usize;
        if size < 40 {
            return Err("Invalid swap chunk size".into());
        }
        let off = u32le(&lv, header + 24)? as usize;
        let data = disc.range(&imgname, off, size - 32)?;
        let table = u32le(&data, 0)? as usize;
        let count = u16le(&data, 4)? as usize;
        if table < 32 {
            return Err("Invalid swap resource table".into());
        }
        for i in 0..count {
            let row = table - 32 + i * 8;
            let id = u32le(&data, row)?;
            if id >= resources as u32 {
                continue;
            }
            let pointer = u32le(&data, row + 4)? as usize;
            if pointer < 32 {
                continue;
            }
            if needed.contains(&(id as u16)) {
                if let Ok(mesh) = geometry(&data, pointer - 32) {
                    meshes.insert(id as u16, mesh);
                    needed.remove(&(id as u16));
                    loaded_swap_geometries += 1;
                }
            } else if !textures.contains_key(&(id as u16)) && !meshes.contains_key(&(id as u16)) {
                if let Ok(texture) = decode_raster(&data, pointer - 32, -32) {
                    textures.insert(id as u16, texture);
                    loaded_swap_textures += 1;
                }
            }
            if meshes.contains_key(&(id as u16)) {
                let conditions = resource_visibility.entry(id as u16).or_default();
                if !conditions.contains(&condition) {
                    conditions.push(condition);
                }
            }
        }
    }
    let mut instance_visibility = HashMap::new();
    let swaps = u32le(&lv, 0x2d8)? as usize;
    let swap_table = u32le(&lv, 0x2dc)? as usize;
    if swaps > 32768 {
        return Err("Invalid level swap count".into());
    }
    for i in 0..swaps {
        let row = swap_table + i * 4;
        instance_visibility.insert(
            u16le(&lv, row + 2)?,
            Visibility {
                off: bytes(&lv, row, 1)?[0],
                on: bytes(&lv, row + 1, 1)?[0],
            },
        );
    }
    for inst in &instances {
        if !meshes.contains_key(&inst.resource) && attempted.insert(inst.resource) {
            unsupported += 1;
        }
    }
    let drawable = instances
        .iter()
        .filter(|i| meshes.contains_key(&i.resource))
        .count();
    let stats = LevelStats {
        name: name.into(),
        inflated_bytes: lv.len(),
        resources,
        sectors,
        instances: instances.len(),
        loaded_meshes: meshes.len(),
        mesh_triangles: meshes.values().map(|m| m.triangles.len()).sum(),
        drawable_instances: drawable,
        unsupported_meshes: unsupported,
        unresolved_instances: instances.len() - drawable,
        loaded_textures: textures.len(),
        textured_mesh_triangles: meshes
            .values()
            .flat_map(|m| m.triangles.iter())
            .filter(|t| textures.contains_key(&t.texture_id))
            .count(),
        loaded_swap_geometries,
        loaded_swap_textures,
        alpha_textures: textures.values().filter(|t| t.has_alpha).count(),
        indexed4_textures: textures.values().filter(|t| t.depth == 4).count(),
        indexed8_textures: textures.values().filter(|t| t.depth == 8).count(),
    };
    Ok(Level {
        stats,
        instances,
        meshes,
        textures,
        resource_visibility,
        instance_visibility,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_float_edge_cases() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xc000), -2.0);
        assert!(half(0x7c00).is_infinite());
        assert!(half(0x7e00).is_nan());
        assert!(half(1) > 0.0);
    }
    #[test]
    fn rejects_invalid_compression() {
        assert!(inflate(b"not zlib").is_err());
    }
    #[test]
    fn rejects_unknown_geometry() {
        assert!(geometry(&[0; 64], 0).is_err());
    }
    #[test]
    fn night_overlay_visibility_wraps_midnight() {
        let night = Visibility {
            off: 128 | 6,
            on: 18,
        };
        assert!(night.allows(23));
        assert!(night.allows(0));
        assert!(!night.allows(12));
        assert!(!night.allows(6));
    }
    #[test]
    fn empty_and_script_visibility_intervals() {
        assert!(!Visibility {
            off: 128 | 12,
            on: 12
        }
        .allows(12));
        assert!(!Visibility {
            off: 128 | 12,
            on: 12
        }
        .allows(0));
        assert!(Visibility { off: 1, on: 0 }.allows(12));
        assert!(!Visibility { off: 1, on: 2 }.allows(12));
        assert!(!Visibility { off: 255, on: 0 }.allows(12));
    }
}
