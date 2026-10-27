//! Native PS2 water grid and particle texture, rendered as a flat surface.
//! GAME+0xa8 owns CWaterLevel: height/rectangle pointers, 64² coarse cells,
//! then 128² fine cells. Bit 7 disables a cell; other values index heights.
//! Grid origin (-2448,-2048), 32m cells and X-major ordering match the VCS
//! format. Character buoyancy queries this same grid; waves remain unimplemented.
use crate::iso::{bytes, f32le, u32le, Result};
use crate::leeds::{Instance, Level, Mesh, Triangle, Vertex};
use crate::texture::Texture;
use serde::Serialize;
use std::collections::BTreeMap;

pub const RENDER_PASS: usize = 8; // Runtime surface, outside the eight native sector lists.
const GRID: usize = 128;
const CELL: f32 = 32.0;
const ORIGIN: [f32; 2] = [-2448.0, -2048.0];

pub struct WaterGrid {
    heights: Vec<f32>,
    cells: Vec<u8>,
}
impl WaterGrid {
    pub fn load(game: &[u8]) -> Result<Self> {
        let base = u32le(game, 0xa8)? as usize;
        if base < 32 {
            return Err("Missing native water data".into());
        }
        let count = u32le(game, base)? as usize;
        if count == 0 || count > 128 {
            return Err("Invalid native water height count".into());
        }
        let table = u32le(game, base + 4)? as usize;
        if table < 32 {
            return Err("Invalid native water height pointer".into());
        }
        let heights = (0..count)
            .map(|i| f32le(game, table + i * 4))
            .collect::<Result<Vec<_>>>()?;
        if heights.iter().any(|z| !z.is_finite() || z.abs() > 1000.0) {
            return Err("Invalid native water height".into());
        }
        let coarse = bytes(game, base + 12, 64 * 64)?;
        let cells = bytes(game, base + 12 + 64 * 64, GRID * GRID)?.to_vec();
        if coarse
            .iter()
            .chain(&cells)
            .any(|&v| v & 128 == 0 && v as usize >= count)
        {
            return Err("Native water cell references an unknown height".into());
        }
        Ok(Self { heights, cells })
    }

    pub fn height(&self, position: [f32; 3]) -> Option<f32> {
        if position.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let x = ((position[0] - ORIGIN[0]) / CELL).floor();
        let y = ((position[1] - ORIGIN[1]) / CELL).floor();
        if x < 0. || y < 0. || x >= GRID as f32 || y >= GRID as f32 {
            return None;
        }
        let id = self.cells[x as usize * GRID + y as usize];
        (id & 128 == 0).then(|| self.heights[id as usize])
    }

    #[cfg(test)]
    pub(crate) fn test_flat(height: f32) -> Self {
        Self {
            heights: vec![height],
            cells: vec![0; GRID * GRID],
        }
    }
}

#[derive(Serialize)]
pub struct WaterStats {
    pub height_levels: Vec<f32>,
    pub cells: usize,
    pub triangles: usize,
    pub patches: usize,
    pub texture_name: &'static str,
    pub texture_size: [usize; 2],
    pub noon_weather_zero_rgba: [u8; 4],
}

fn tile(x: usize, y: usize, z: f32, texture_id: u16) -> [Triangle; 2] {
    let x = ORIGIN[0] + x as f32 * CELL;
    let y = ORIGIN[1] + y as f32 * CELL;
    let vertex = |dx: f32, dy: f32| Vertex {
        p: [x + dx * CELL, y + dy * CELL, z],
        color: [1.0; 3],
        uv: [dx, dy],
    };
    let p = [
        vertex(0.0, 0.0),
        vertex(1.0, 0.0),
        vertex(1.0, 1.0),
        vertex(0.0, 1.0),
    ];
    [
        Triangle {
            vertices: [p[0], p[1], p[2]],
            texture_id,
        },
        Triangle {
            vertices: [p[0], p[2], p[3]],
            texture_id,
        },
    ]
}

fn next_resource(level: &Level, start: u16) -> Result<u16> {
    (start..=u16::MAX)
        .find(|id| !level.meshes.contains_key(id) && !level.textures.contains_key(id))
        .ok_or_else(|| "No resource ID available for native water".into())
}

/// Attach only to the outdoor owning archive. No collision surfaces are added.
pub fn attach(game: &[u8], level: &mut Level) -> Result<WaterStats> {
    let grid = WaterGrid::load(game)?;
    let texture = Texture::load_embedded_named(game, "particle", "waterclear256")?;
    // CTimeCycle's four water arrays start at +0x2980, each 24 hours × 8 weathers.
    // The scene is currently fixed at noon; use native weather index zero too.
    let cycle = u32le(game, 0xb4)? as usize;
    if cycle < 32 {
        return Err("Missing native timecycle".into());
    }
    let mut tint = [0; 4];
    for (channel, value) in tint.iter_mut().enumerate() {
        *value = bytes(game, cycle + 0x2980 + channel * 192 + 12 * 8, 1)?[0];
    }
    let mut stats = WaterStats {
        height_levels: grid.heights.clone(),
        cells: 0,
        triangles: 0,
        patches: 0,
        texture_name: "particle/waterclear256",
        texture_size: [texture.width, texture.height],
        noon_weather_zero_rgba: tint,
    };
    let texture_id = next_resource(level, level.stats.resources.try_into()?)?;
    level.textures.insert(texture_id, texture.tinted(tint));
    let mut patches = BTreeMap::<(usize, usize), Vec<Triangle>>::new();
    for x in 0..GRID {
        for y in 0..GRID {
            let id = grid.cells[x * GRID + y];
            if id & 128 != 0 {
                continue;
            }
            patches.entry((x / 8, y / 8)).or_default().extend(tile(
                x,
                y,
                grid.heights[id as usize],
                texture_id,
            ));
            stats.cells += 1;
        }
    }
    let mut resource = texture_id;
    for (_, triangles) in patches {
        resource = next_resource(level, resource)?;
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for vertex in triangles.iter().flat_map(|t| t.vertices) {
            for a in 0..3 {
                lo[a] = lo[a].min(vertex.p[a]);
                hi[a] = hi[a].max(vertex.p[a]);
            }
        }
        let center: [f32; 3] = std::array::from_fn(|a| (lo[a] + hi[a]) * 0.5);
        let radius = (0..3).map(|a| (hi[a] - lo[a]).powi(2)).sum::<f32>().sqrt() * 0.5;
        let mut matrix = [0.0; 16];
        for i in [0, 5, 10, 15] {
            matrix[i] = 1.0;
        }
        level.instances.push(Instance {
            id: u16::MAX,
            resource,
            pass: RENDER_PASS,
            matrix,
            sphere: [center[0], center[1], center[2], radius],
        });
        stats.triangles += triangles.len();
        stats.patches += 1;
        level.meshes.insert(resource, Mesh { triangles });
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 0x100 + 12 + 4096 + 16384 + 4];
        b[0xa8..0xac].copy_from_slice(&0x100u32.to_le_bytes());
        b[0x100..0x104].copy_from_slice(&1u32.to_le_bytes());
        b[0x104..0x108].copy_from_slice(&0xe0u32.to_le_bytes());
        b[0xe0..0xe4].copy_from_slice(&6f32.to_le_bytes());
        b[0x10c..].fill(128);
        b[0x10c + 4096 + 3 * 128 + 7] = 0;
        b
    }
    #[test]
    fn fine_grid_uses_native_height_and_disabled_bit() {
        let grid = WaterGrid::load(&fixture()).unwrap();
        assert_eq!(grid.heights, vec![6.0]);
        assert_eq!(grid.cells.iter().filter(|&&c| c & 128 == 0).count(), 1);
        assert_eq!(grid.cells[3 * 128 + 7], 0);
        assert_eq!(grid.height([-2351., -1823., 100.]), Some(6.));
        assert_eq!(grid.height([-2321., -1793., -100.]), Some(6.));
        assert_eq!(grid.height([-2320., -1793., 6.]), None);
        assert_eq!(grid.height([f32::NAN, 0., 0.]), None);
        assert_eq!(grid.height([-3000., 0., 0.]), None);
        let mesh = tile(3, 7, grid.heights[0], 9);
        assert_eq!(mesh[0].vertices[0].p, [-2352.0, -1824.0, 6.0]);
        assert_eq!(mesh[1].vertices[1].p, [-2320.0, -1792.0, 6.0]);
        assert_eq!(mesh[1].texture_id, 9);
    }
    #[test]
    fn invalid_water_indices_heights_and_truncation_fail() {
        assert!(WaterGrid::load(&[0; 32]).is_err());
        let mut b = fixture();
        b[0x10c + 4096] = 1;
        assert!(WaterGrid::load(&b).is_err());
        let mut b = fixture();
        b[0xe0..0xe4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(WaterGrid::load(&b).is_err());
        let mut b = fixture();
        b.truncate(0x200);
        assert!(WaterGrid::load(&b).is_err());
    }

}
