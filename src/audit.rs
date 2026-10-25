//! Read-only comparison of displayed map geometry and static collision contacts.
//! Differences are investigation candidates: collision meshes are deliberately
//! simpler than visual meshes, so this does not classify every mismatch as a bug.
use crate::collision::{CollisionWorld, Triangle};
use crate::leeds::Level;
use crate::paths::PathNetwork;
use std::collections::{BTreeMap, HashSet};

fn visual_world(levels: &[Level], include_terrain: bool) -> CollisionWorld {
    let mut triangles = Vec::new();
    for (level_index, instance_index) in crate::scene::unique_instances(levels, 0) {
        let level = &levels[level_index];
        let instance = &level.instances[instance_index];
        if !matches!(instance.pass, 1 | 3 | 4 | 5 | 7) || (!include_terrain && instance.pass == 1) {
            continue;
        }
        if let Some(mesh) = level.meshes.get(&instance.resource) {
            let m = instance.matrix;
            triangles.extend(mesh.triangles.iter().map(|tri| Triangle {
                vertices: tri.vertices.map(|v| {
                    std::array::from_fn(|a| {
                        m[a] * v.p[0] + m[4 + a] * v.p[1] + m[8 + a] * v.p[2] + m[12 + a]
                    })
                }),
                surface: 0,
                model_id: instance.resource,
            }));
        }
    }
    CollisionWorld::from_triangles(triangles)
}

pub fn map_visibility(
    levels: &[Level],
    world: &mut CollisionWorld,
    paths: &PathNetwork,
) -> serde_json::Value {
    crate::scene::select_collisions(world, 0);
    let mut origins: Vec<_> = paths
        .nodes
        .iter()
        .map(|n| [n.position[0], n.position[1], n.position[2] + 3.0])
        .collect();
    // Probe courtyards/terrain too: pedestrians' routes do not cover every yard.
    for level in levels.iter().take(2) {
        for inst in level
            .instances
            .iter()
            .filter(|i| i.pass == 1 && level.visible(i, 12))
        {
            let [x, y, z, r] = inst.sphere;
            let r = r.clamp(0.0, 120.0);
            let steps = (r / 12.0).ceil() as i32;
            for dx in -steps..=steps {
                for dy in -steps..=steps {
                    if (dx * dx + dy * dy) as f32 * 144.0 <= r * r {
                        origins.push([x + dx as f32 * 12.0, y + dy as f32 * 12.0, z + 3.0]);
                    }
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let points: Vec<_> = origins
        .into_iter()
        .filter_map(|origin| {
            let hit = world.raycast(origin, [0.0, 0.0, -1.0], 8.0)?;
            if hit.normal[2] < 0.70 {
                return None;
            }
            let key = hit.position.map(|v| (v * 2.0).round() as i32);
            seen.insert(key).then_some(hit)
        })
        .collect();
    let before = visual_world(levels, false);
    let previously_missing: Vec<_> = points
        .iter()
        .map(|p| {
            let hit = before.raycast(
                [p.position[0], p.position[1], p.position[2] + 0.65],
                [0.0, 0.0, -1.0],
                1.30,
            );
            hit.is_none()
        })
        .collect();
    drop(before);
    let visible = visual_world(levels, true);
    let mut restored = 0usize;
    let mut missing = Vec::new();
    let mut walls = Vec::new();
    let mut wall_cells = HashSet::new();
    let mut wall_counts = BTreeMap::<u16, usize>::new();
    for (index, point) in points.iter().enumerate() {
        let pos = point.position;
        let hit = visible.raycast([pos[0], pos[1], pos[2] + 0.65], [0.0, 0.0, -1.0], 1.30);
        if hit.is_none() {
            missing.push(serde_json::json!({"position":pos,"collision_model":point.model_id}));
        } else if previously_missing[index] {
            restored += 1;
        }
        // Eight short horizontal probes from each accessible sample.
        for i in 0..8 {
            let angle = i as f32 * std::f32::consts::TAU / 8.0;
            let (s, c) = angle.sin_cos();
            let origin = [pos[0], pos[1], pos[2] + 1.0];
            let dir = [s, c, 0.0];
            if let Some(contact) = world.raycast(origin, dir, 8.0) {
                if contact.normal[2].abs() > 0.55 {
                    continue;
                }
                let rendered = visible.raycast(origin, dir, contact.distance + 0.75);
                if rendered.is_none() {
                    *wall_counts.entry(contact.model_id).or_default() += 1;
                    // Keep coverage across the whole map instead of stopping at
                    // the first 200 contacts (mostly trees along the first routes).
                    let cell = contact.position.map(|v| (v / 2.0).floor() as i32);
                    if wall_cells.insert((contact.model_id, cell)) {
                        walls.push(serde_json::json!({"position":contact.position,"from":origin,"collision_model":contact.model_id}));
                    }
                }
            }
        }
    }
    serde_json::json!({
        "scope":"Both outdoor islands; native path nodes and 12m samples around terrain, noon/default state",
        "ground_samples":points.len(),"missing_before":previously_missing.iter().filter(|v|**v).count(),
        "restored_by_terrain_pass":restored,"remaining_ground_candidates":missing,
        "wall_candidate_counts_by_model":wall_counts,
        "wall_candidates":walls,
        "limitations":"Geometry comparison; not texture-alpha coverage, not missions/interiors; collision/detail tolerances can produce false positives",
    })
}
