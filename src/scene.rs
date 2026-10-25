//! Continuous outdoor scene. Resource IDs remain local to their owning level.
use crate::collision::CollisionWorld;
use crate::leeds::{Instance, Level, Mesh};
use crate::render::Camera;
use crate::texture::Texture;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

type Fingerprint = [u8; 32];

#[derive(PartialEq, Eq, Hash)]
struct PlacementKey {
    pass: usize,
    sphere: [u32; 4],
    matrix: [u32; 12],
    mesh: Fingerprint,
}

fn texture_fingerprint(texture: &Texture) -> Fingerprint {
    let mut hash = Sha256::new();
    hash.update(b"VCS_RGBA_MIPS_V1");
    hash.update((texture.mip_levels() as u32).to_le_bytes());
    for mip in 0..texture.mip_levels() {
        // mip_levels() counts the base image and every complete native/generated mip.
        let (width, height, rgba) = texture.mip(mip).expect("Texture mip inventory");
        hash.update((width as u32).to_le_bytes());
        hash.update((height as u32).to_le_bytes());
        hash.update(rgba);
    }
    hash.finalize().into()
}

fn mesh_fingerprint(mesh: &Mesh, textures: &HashMap<u16, Fingerprint>) -> Option<Fingerprint> {
    let mut hash = Sha256::new();
    hash.update(b"VCS_VISUAL_MESH_V1");
    hash.update((mesh.triangles.len() as u64).to_le_bytes());
    for triangle in &mesh.triangles {
        // Material IDs are local to their archive. Only the decoded pixel data is semantic.
        hash.update(textures.get(&triangle.texture_id)?);
        for vertex in triangle.vertices {
            for value in vertex.p.into_iter().chain(vertex.color).chain(vertex.uv) {
                hash.update(value.to_bits().to_le_bytes());
            }
        }
    }
    Some(hash.finalize().into())
}

fn placement_key(instance: &Instance, mesh: Fingerprint) -> PlacementKey {
    let matrix = std::array::from_fn(|index| {
        // Fourth matrix components contain PS2 runtime flags; renderers replace them.
        instance.matrix[(index / 3) * 4 + index % 3].to_bits()
    });
    PlacementKey {
        pass: instance.pass,
        sphere: instance.sphere.map(f32::to_bits),
        matrix,
        mesh,
    }
}

/// Builds the initial visible scene once, removing exact copies from other archives.
/// Instance/material IDs are archive-local and are deliberately absent from the key.
/// The key preserves native bounds, render pass, transform, vertices, UVs, prelight and
/// every texture mip. Resources without a complete fingerprint remain in the scene.
pub fn unique_instances(levels: &[Level], zone: usize) -> Vec<(usize, usize)> {
    let mut seen = HashMap::new();
    let mut instances = Vec::new();
    for &level_index in level_indices(zone) {
        let Some(level) = levels.get(level_index) else {
            continue;
        };
        let textures: HashMap<_, _> = level
            .textures
            .iter()
            .map(|(&id, texture)| (id, texture_fingerprint(texture)))
            .collect();
        let meshes: HashMap<_, _> = level
            .meshes
            .iter()
            .filter_map(|(&id, mesh)| mesh_fingerprint(mesh, &textures).map(|hash| (id, hash)))
            .collect();
        for (instance_index, instance) in level.instances.iter().enumerate() {
            if !level.visible(instance, 12) {
                continue;
            }
            if let Some(&fingerprint) = meshes.get(&instance.resource) {
                let key = placement_key(instance, fingerprint);
                if let Some(&owner) = seen.get(&key) {
                    if owner != level_index {
                        continue;
                    }
                } else {
                    seen.insert(key, level_index);
                }
            }
            instances.push((level_index, instance_index));
        }
    }
    instances
}

pub const DRAW_DISTANCE: f32 = 5000.0;
pub fn level_indices(zone: usize) -> &'static [usize] {
    if zone == 2 {
        &[2]
    } else {
        &[0, 1]
    }
}
pub fn label(zone: usize) -> &'static str {
    if zone == 2 {
        "MALL"
    } else {
        "VICE CITY"
    }
}
pub fn select_collisions(world: &mut CollisionWorld, zone: usize) {
    world.set_active_levels(if zone == 2 { &[3] } else { &[1, 2] });
}
pub fn is_blended(instance: &Instance) -> bool {
    matches!(instance.pass, 5 | 7) || instance.pass == crate::water::RENDER_PASS
}
pub fn center(instance: &Instance) -> [f32; 3] {
    if instance.sphere[..3].iter().all(|x| x.is_finite()) {
        instance.sphere[..3].try_into().unwrap()
    } else {
        [
            instance.matrix[12],
            instance.matrix[13],
            instance.matrix[14],
        ]
    }
}
pub fn distance_squared(instance: &Instance, camera: &Camera) -> f32 {
    center(instance)
        .iter()
        .zip(camera.pos)
        .map(|(x, y)| (x - y).powi(2))
        .sum()
}
pub fn instance_in_view(instance: &Instance, camera: &Camera) -> bool {
    // VCS pass 1 contains terrain that can extend above the waterline too.
    // Excluding the entire pass leaves walkable courtyards/shore ground absent.
    if !matches!(instance.pass, 1 | 3 | 4 | 5 | 7) && instance.pass != crate::water::RENDER_PASS {
        return false;
    }
    let center = center(instance);
    let radius = if instance.sphere[3].is_finite() {
        instance.sphere[3].abs()
    } else {
        DRAW_DISTANCE
    };
    if distance_squared(instance, camera) > (DRAW_DISTANCE + radius).powi(2) {
        return false;
    }
    let d = std::array::from_fn::<_, 3, _>(|i| center[i] - camera.pos[i]);
    let (s, c) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    let x = c * d[0] - s * d[1];
    let z = s * d[0] + c * d[1];
    let forward = sp * d[2] + cp * z;
    forward >= -radius && x.abs() <= forward.max(0.0) * 1.5 + radius
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instance(y: f32, radius: f32, pass: usize) -> Instance {
        Instance {
            id: 0,
            resource: 0,
            pass,
            matrix: [0.0; 16],
            sphere: [0.0, y, 0.0, radius],
        }
    }
    #[test]
    fn outdoor_destinations_include_both_islands_but_not_mall() {
        assert_eq!(level_indices(0), &[0, 1]);
        assert_eq!(level_indices(1), &[0, 1]);
        assert_eq!(level_indices(2), &[2]);
    }
    #[test]
    fn distant_mainland_and_large_ground_bounds_remain_visible() {
        let camera = Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        };
        assert!(instance_in_view(&instance(3000.0, 10.0, 4), &camera));
        assert!(instance_in_view(&instance(5500.0, 700.0, 3), &camera));
        assert!(!instance_in_view(&instance(5500.0, 10.0, 4), &camera));
        assert!(!instance_in_view(&instance(-1000.0, 20.0, 4), &camera));
    }
    #[test]
    fn fences_render_without_enabling_duplicate_lod_or_day_lights() {
        let camera = Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        };
        assert!(instance_in_view(&instance(20.0, 10.0, 7), &camera));
        assert!(instance_in_view(&instance(20.0, 60.0, 1), &camera));
        assert!(is_blended(&instance(20.0, 10.0, 5)));
        let water = instance(20.0, 200.0, crate::water::RENDER_PASS);
        assert!(instance_in_view(&water, &camera));
        assert!(is_blended(&water));
        assert!(!instance_in_view(&instance(20.0, 10.0, 2), &camera));
        assert!(!instance_in_view(&instance(20.0, 10.0, 6), &camera));
    }

    #[test]
    fn exact_visual_placement_ignores_archive_local_ids_and_padding() {
        let mut a = instance(20.0, 10.0, 4);
        a.matrix[0] = 1.0;
        a.matrix[5] = 1.0;
        a.matrix[10] = 1.0;
        a.matrix[12] = 5.0;
        let mut b = a.clone();
        b.id = 700;
        b.resource = 1200;
        b.matrix[3] = f32::from_bits(0x11223344);
        b.matrix[15] = f32::from_bits(0x88776655);
        assert!(placement_key(&a, [1; 32]) == placement_key(&b, [1; 32]));
        b.matrix[12] += 0.0001;
        assert!(placement_key(&a, [1; 32]) != placement_key(&b, [1; 32]));
        b.matrix[12] = a.matrix[12];
        b.pass = 7;
        assert!(placement_key(&a, [1; 32]) != placement_key(&b, [1; 32]));
        b.pass = a.pass;
        assert!(placement_key(&a, [1; 32]) != placement_key(&b, [2; 32]));
    }

    #[test]
    fn visual_mesh_hash_uses_texture_pixels_and_preserves_uv_prelight() {
        use crate::leeds::{Triangle, Vertex};
        let vertex = Vertex {
            p: [0.0, 0.0, 0.0],
            color: [1.0; 3],
            uv: [0.0, 0.0],
        };
        let mesh = Mesh {
            triangles: vec![Triangle {
                vertices: [vertex; 3],
                texture_id: 1,
            }],
        };
        let mut textures = HashMap::from([(1, [7; 32]), (50, [7; 32])]);
        let baseline = mesh_fingerprint(&mesh, &textures).unwrap();
        let mut other = mesh.clone();
        other.triangles[0].texture_id = 50;
        assert_eq!(Some(baseline), mesh_fingerprint(&other, &textures));
        textures.insert(50, [8; 32]);
        assert_ne!(Some(baseline), mesh_fingerprint(&other, &textures));
        textures.insert(50, [7; 32]);
        other.triangles[0].vertices[0].uv[0] = 0.25;
        assert_ne!(Some(baseline), mesh_fingerprint(&other, &textures));
        other.triangles[0].vertices[0].uv[0] = 0.0;
        other.triangles[0].vertices[0].color[0] = 0.5;
        assert_ne!(Some(baseline), mesh_fingerprint(&other, &textures));
        textures.remove(&50);
        assert_eq!(None, mesh_fingerprint(&other, &textures));
    }
}
