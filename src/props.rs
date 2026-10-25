//! Street furniture omitted from baked sectors: original DUMMY placements and
//! their level-local sDynamic resources. Never instantiate every dynamic row:
//! alternate rows are LODs and many BUILDING props already belong to sectors.
use crate::iso::{bytes, f32le, u16le, u32le, Disc, Result};
use crate::leeds::{Instance, Level, Mesh};
use serde::Serialize;
use std::collections::BTreeMap;

// Verified CBaseModelInfo name hashes, not assumed model IDs. Restrict this
// initial restoration to the seven furniture families inspected in the ISO.
const NAMES: [(u32, &str); 7] = [
    (0x19ac2b70, "lamppost3"),
    (0xcdabfe15, "streetlamp2"),
    (0x54a2afaf, "streetlamp1"),
    (0x8f6d7643, "parkbench1"),
    (0x6eab1be6, "lamppost2"),
    (0xf7a24a5c, "lamppost1"),
    (0x6ceb2e32, "mlamppost"),
];

/// SLES 0x131624..0x1316c4 sets object flag bit 25 by this native model
/// catalog, NOT by special collision response (+0x1da). The catalog pointer
/// is GAME+0x5c (loader 0x332564). Streetlamp1/2 are not in this list on PAL.
fn ignores_actors_when_tilted(game: &[u8], model: u16) -> Result<bool> {
    let catalog = u32le(game, 0x5c)? as usize;
    if catalog < 32 {
        return Err("Missing native object model catalog".into());
    }
    for offset in [6, 0xe2, 0xa, 0xc, 0xe, 0x10, 8, 2, 0xba, 0xbc] {
        if u16le(game, catalog + offset)? == model {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Serialize, Default)]
pub struct PropStats {
    pub restored: usize,
    pub by_model: BTreeMap<String, usize>,
    pub already_present: usize,
    pub original_physics: BTreeMap<String, ObjectParameters>,
    #[serde(skip)]
    pub bindings: Vec<crate::objects::Binding>,
}

/// Proven PS2 layout, not the 32-byte layout described by some external tools.
/// SLES 0x332594 stores ResourceImage+0x54 (GAME+0x74) at 0x488354.
/// 0x2d8694..0x2d8730 indexes modelInfo+0x1a by 0x24 bytes and copies
/// +0 into physical+0xd0 (mass), +4 into +0xd4, through +0x18 into +0x1cc,
/// followed by five separate bytes. Unverified fields retain raw names.
#[derive(Serialize)]
pub struct ObjectParameters {
    pub record_index: u16,
    pub mass: f32,
    pub raw_floats_04_to_18: [f32; 6],
    pub raw_bytes_1c_to_20: [u8; 5],
}

impl ObjectParameters {
    fn load(game: &[u8], info: usize) -> Result<Option<Self>> {
        let index = u16le(game, info + 0x1a)? as i16;
        if index == -1 {
            return Ok(None);
        }
        if index < 0 {
            return Err("Invalid object physics index".into());
        }
        let table = u32le(game, 0x74)? as usize;
        if table < 32 {
            return Err("Missing object physics table".into());
        }
        let row = table + index as usize * 36;
        bytes(game, row, 36)?;
        let mass = f32le(game, row)?;
        let mut fields = [0.0; 6];
        for (i, v) in fields.iter_mut().enumerate() {
            *v = f32le(game, row + 4 + i * 4)?;
        }
        if !mass.is_finite() || mass <= 0.0 || fields.iter().any(|v| !v.is_finite()) {
            return Err("Invalid native object physics values".into());
        }
        Ok(Some(Self {
            record_index: index as u16,
            mass,
            raw_floats_04_to_18: fields,
            raw_bytes_1c_to_20: bytes(game, row + 28, 5)?.try_into()?,
        }))
    }
}

struct Dynamic {
    model: u16,
    resource: u16,
    scale: [f32; 3],
    position: [f32; 3],
}

fn dynamics(data: &[u8]) -> Result<Vec<Dynamic>> {
    let count = u32le(data, 0x2e8)? as usize;
    let table = u32le(data, 0x2ec)? as usize;
    if count > 65536 || (count > 0 && table < 32) {
        return Err("Invalid native dynamic resource table".into());
    }
    bytes(data, table, count * 48)?;
    let mut result = Vec::new();
    for i in 0..count {
        let row = table + i * 48;
        // Low byte selects the original simple model element (LOD 0 is the
        // detailed intact object). These verified furniture rows have 0x400.
        if u16le(data, row + 28)? != 0x400 {
            continue;
        }
        let scale = [
            f32le(data, row)?,
            f32le(data, row + 4)?,
            f32le(data, row + 8)?,
        ];
        let position = [
            f32le(data, row + 16)?,
            f32le(data, row + 20)?,
            f32le(data, row + 24)?,
        ];
        if scale.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || position.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid native dynamic resource transform".into());
        }
        result.push(Dynamic {
            model: u16le(data, row + 12)?,
            resource: u16le(data, row + 14)?,
            scale,
            position,
        });
    }
    Ok(result)
}

fn point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|a| m[a] * p[0] + m[4 + a] * p[1] + m[8 + a] * p[2] + m[12 + a])
}

fn placement(entity: &[f32; 16], dynamic: &Dynamic, mesh: &Mesh) -> Result<Instance> {
    let mut matrix = *entity;
    for a in 0..3 {
        for r in 0..3 {
            matrix[a * 4 + r] *= dynamic.scale[a];
        }
        matrix[a * 4 + 3] = 0.0;
    }
    matrix[12..15].copy_from_slice(&point(entity, dynamic.position));
    matrix[15] = 1.0;
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for vertex in mesh.triangles.iter().flat_map(|t| t.vertices) {
        let p = point(&matrix, vertex.p);
        if p.iter().any(|v| !v.is_finite()) {
            return Err("Invalid furniture vertex".into());
        }
        for a in 0..3 {
            low[a] = low[a].min(p[a]);
            high[a] = high[a].max(p[a]);
        }
    }
    if mesh.triangles.is_empty() {
        return Err("Empty furniture mesh".into());
    }
    let center: [f32; 3] = std::array::from_fn(|a| (low[a] + high[a]) * 0.5);
    let radius = (0..3)
        .map(|a| (high[a] - low[a]).powi(2))
        .sum::<f32>()
        .sqrt()
        * 0.5;
    Ok(Instance {
        id: u16::MAX,
        resource: dynamic.resource,
        pass: 4,
        matrix,
        sphere: [center[0], center[1], center[2], radius],
    })
}

pub fn attach(disc: &mut Disc, game: &[u8], levels: &mut [Level]) -> Result<PropStats> {
    let count = u32le(game, 0x38)? as usize;
    let table = u32le(game, 0x3c)? as usize;
    if count > 65536 || table < 32 {
        return Err("Invalid furniture model table".into());
    }
    bytes(game, table, count * 4)?;
    let mut selected = BTreeMap::new();
    let mut stats = PropStats::default();
    for id in 0..count {
        let info = u32le(game, table + id * 4)? as usize;
        if info == 0 {
            continue;
        }
        let hash = u32le(game, info + 8)?;
        if let Some((_, name)) = NAMES.iter().find(|(key, _)| *key == hash) {
            selected.insert(id as u16, *name);
            if let Some(parameters) = ObjectParameters::load(game, info)? {
                stats.original_physics.insert(name.to_string(), parameters);
            }
        }
    }
    let pool = u32le(game, 0x2c)? as usize; // DUMMY only; BUILDING props are baked.
    let items = u32le(game, pool)? as usize;
    let flags = u32le(game, pool + 4)? as usize;
    let count = u32le(game, pool + 8)? as usize;
    if count > 200000 {
        return Err("Invalid furniture placement count".into());
    }
    if count == 0 {
        return Ok(PropStats::default());
    }
    bytes(game, items, count * 96)?;
    bytes(game, flags, count)?;
    for (level_index, level) in levels.iter_mut().enumerate() {
        let zone = match level.stats.name.as_str() {
            "BEACH" => 1,
            "MAINLA" => 2,
            "MALL" => 3,
            _ => continue,
        };
        let lv = crate::leeds::inflate(&disc.read(&format!("{}.LVZ", level.stats.name))?)?;
        let mut resources = BTreeMap::new();
        for dynamic in dynamics(&lv)? {
            if selected.contains_key(&dynamic.model) {
                let model = dynamic.model;
                if resources.insert(model, dynamic).is_some() {
                    return Err("Ambiguous furniture resource".into());
                }
            }
        }
        for i in 0..count {
            let row = items + i * 96;
            if game[flags + i] & 128 != 0 || bytes(game, row + 0x5a, 1)?[0] != zone {
                continue;
            }
            let model = u16le(game, row + 0x56)?;
            let Some(name) = selected.get(&model) else {
                continue;
            };
            let dynamic = resources
                .get(&model)
                .ok_or("Missing furniture resource mapping")?;
            let mesh = level
                .meshes
                .get(&dynamic.resource)
                .ok_or("Missing furniture mesh")?;
            if mesh
                .triangles
                .iter()
                .any(|t| !level.textures.contains_key(&t.texture_id))
            {
                return Err("Missing furniture texture".into());
            }
            let mut entity = [0.0; 16];
            for c in 0..4 {
                for a in 0..3 {
                    entity[c * 4 + a] = f32le(game, row + c * 16 + a * 4)?;
                }
            }
            if entity.iter().any(|v| !v.is_finite()) {
                return Err("Invalid furniture placement matrix".into());
            }
            entity[15] = 1.0;
            let inst = placement(&entity, dynamic, mesh)?;
            let existing = level
                .instances
                .iter()
                .position(|old| old.resource == inst.resource && old.matrix == inst.matrix);
            let instance = existing.unwrap_or(level.instances.len());
            if let Some(parameters) = stats.original_physics.get(*name) {
                let info = u32le(game, table + model as usize * 4)? as usize;
                let collision = u32le(game, info + 0x14)? as usize;
                let radius = f32le(game, collision + 0xc)?;
                if !radius.is_finite() || radius <= 0. {
                    return Err("Invalid native object radius".into());
                }
                stats.bindings.push(crate::objects::Binding {
                    entity: row,
                    level: level_index,
                    instance,
                    mass: parameters.mass,
                    pole: *name != "parkbench1",
                    pivot: [entity[12], entity[13], entity[14]],
                    original_up: [entity[8], entity[9], entity[10]],
                    ignore_when_tilted: ignores_actors_when_tilted(game, model)?,
                    restore_radius: radius,
                });
            }
            if existing.is_some() {
                stats.already_present += 1;
                continue;
            }
            level.instances.push(inst);
            stats.restored += 1;
            *stats.by_model.entry(name.to_string()).or_default() += 1;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires the user's local VCS PS2 PAL ISO"]
    fn native_furniture_placement_is_textured_and_idempotent() {
        let mut disc = Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut levels = vec![
            crate::leeds::load_level(&mut disc, "BEACH").unwrap(),
            crate::leeds::load_level(&mut disc, "MAINLA").unwrap(),
        ];
        let first = attach(&mut disc, &game, &mut levels).unwrap();
        assert_eq!(first.restored, 1028);
        assert_eq!(first.by_model["parkbench1"], 54);
        assert_eq!(first.original_physics["parkbench1"].mass, 50.0);
        assert_eq!(first.original_physics["lamppost2"].mass, 1250.0);
        for model in [451, 516, 524, 528] {
            assert!(ignores_actors_when_tilted(&game, model).unwrap());
        }
        for model in [457, 458, 513] {
            assert!(!ignores_actors_when_tilted(&game, model).unwrap());
        }
        for level in &levels {
            let mut seen = std::collections::HashSet::new();
            for inst in level.instances.iter().filter(|i| i.id == u16::MAX) {
                let mesh = &level.meshes[&inst.resource];
                assert!(level.visible(inst, 12));
                assert!(mesh
                    .triangles
                    .iter()
                    .all(|t| level.textures.contains_key(&t.texture_id)));
                if seen.insert(inst.resource) {
                    let colors: Vec<_> = mesh
                        .triangles
                        .iter()
                        .flat_map(|t| t.vertices)
                        .flat_map(|v| v.color)
                        .collect();
                    println!(
                        "{} furniture {} color range {:?}",
                        level.stats.name,
                        inst.resource,
                        [
                            colors.iter().copied().fold(f32::INFINITY, f32::min),
                            colors.iter().copied().fold(0.0, f32::max)
                        ]
                    );
                }
            }
        }
        let before: usize = levels.iter().map(|l| l.instances.len()).sum();
        let second = attach(&mut disc, &game, &mut levels).unwrap();
        assert_eq!(second.restored, 0);
        assert_eq!(second.already_present, 1029);
        assert_eq!(
            levels.iter().map(|l| l.instances.len()).sum::<usize>(),
            before
        );
        println!(
            "Native furniture: {} restored, second attachment adds none",
            first.restored
        );
    }
    #[test]
    fn object_physics_uses_proven_36_byte_stride_and_checks_missing_records() {
        let mut game = vec![0; 0x400];
        game[0x74..0x78].copy_from_slice(&0x200u32.to_le_bytes());
        game[0x11a..0x11c].copy_from_slice(&2u16.to_le_bytes());
        let row = 0x200 + 2 * 36;
        game[row..row + 4].copy_from_slice(&50f32.to_le_bytes());
        game[row + 4..row + 8].copy_from_slice(&100f32.to_le_bytes());
        game[row + 28..row + 33].copy_from_slice(&[1, 2, 3, 4, 5]);
        let p = ObjectParameters::load(&game, 0x100).unwrap().unwrap();
        assert_eq!(p.mass, 50.0);
        assert_eq!(p.raw_floats_04_to_18[0], 100.0);
        assert_eq!(p.raw_bytes_1c_to_20, [1, 2, 3, 4, 5]);
        game[0x11a..0x11c].copy_from_slice(&(-1i16).to_le_bytes());
        assert!(ObjectParameters::load(&game, 0x100).unwrap().is_none());
        game[0x11a..0x11c].copy_from_slice(&2u16.to_le_bytes());
        game.truncate(row + 32);
        assert!(ObjectParameters::load(&game, 0x100).is_err());
    }
    #[test]
    fn dynamic_detail_selection_and_truncation() {
        let mut b = vec![0; 0x400];
        b[0x2e8..0x2ec].copy_from_slice(&2u32.to_le_bytes());
        b[0x2ec..0x2f0].copy_from_slice(&0x300u32.to_le_bytes());
        for row in [0x300, 0x330] {
            for a in 0..3 {
                b[row + a * 4..row + a * 4 + 4].copy_from_slice(&1f32.to_le_bytes());
            }
        }
        b[0x31c..0x31e].copy_from_slice(&0x400u16.to_le_bytes());
        b[0x34c..0x34e].copy_from_slice(&0x401u16.to_le_bytes());
        assert_eq!(dynamics(&b).unwrap().len(), 1);
        b.truncate(0x320);
        assert!(dynamics(&b).is_err());
    }
    #[test]
    fn placement_uses_native_scale_offset_rotation_and_bounds() {
        use crate::leeds::{Triangle, Vertex};
        let d = Dynamic {
            model: 451,
            resource: 8,
            scale: [2.0, 0.25, 4.0],
            position: [0.5, 0.25, 1.0],
        };
        let entity = [
            0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 20.0, 30.0, 1.0,
        ];
        let mesh = Mesh {
            triangles: vec![Triangle {
                texture_id: 1,
                vertices: [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 1.0]].map(|p| Vertex {
                    p,
                    uv: [0.0; 2],
                    color: [1.0; 3],
                }),
            }],
        };
        let i = placement(&entity, &d, &mesh).unwrap();
        assert_eq!(point(&i.matrix, [0.0; 3]), [10.25, 19.5, 31.0]);
        assert_eq!(point(&i.matrix, [1.0, 0.0, 0.0]), [10.25, 17.5, 31.0]);
        assert_eq!(point(&i.matrix, [0.0, 1.0, 1.0]), [10.5, 19.5, 35.0]);
        for v in mesh.triangles[0].vertices {
            let p = point(&i.matrix, v.p);
            assert!(
                (0..3)
                    .map(|a| (p[a] - i.sphere[a]).powi(2))
                    .sum::<f32>()
                    .sqrt()
                    <= i.sphere[3] + 0.0001
            );
        }
    }
}
