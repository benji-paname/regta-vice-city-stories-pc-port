//! Native VCS PS2 collision reader. Serialized pointers stay checked chunk offsets.
//!
//! Layouts are decoded from the 32-bit VCS structures, not host C layouts:
//! CColModel=0x60, CEntity=0x60, compressed vertex=i16 XYZ /128, triangle=8 bytes.
//! The COL2 signature on this disc is the little-endian integer `col2` (`2loc`).
//! Only original collision triangles/boxes/spheres generate contacts. The model's
//! bounding box is never treated as a collision surface.
use crate::iso::{bytes, f32le, u16le, u32le, Disc, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

type Vec3 = [f32; 3];
const CELL: f32 = 32.0;
const MAX_MODELS: usize = 65536;
const MAX_POOL: usize = 200000;

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub surface: u8,
    pub model_id: u16,
}

#[derive(Clone, Copy, Debug)]
struct Sphere {
    center: Vec3,
    radius: f32,
    // Inverse linear transform and world origin let rays hit the exact ellipsoid
    // if a placement has nonuniform scale rather than guessing a world radius.
    inverse: [[f32; 3]; 3],
    origin: Vec3,
    bounds: [Vec3; 2],
    surface: u8,
    model_id: u16,
    level: u8,
    entity: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Physical distance along the normalized world ray.
    pub distance: f32,
    /// Serialized placement address, absent for actor/test shapes.
    pub entity: Option<usize>,
    pub position: Vec3,
    /// Unit surface normal, facing against the incoming ray (double-sided).
    pub normal: Vec3,
    pub surface: u8,
    pub model_id: u16,
}

/// Shared query interface for the static map and moving instances. Driving
/// still queries the static world, so a vehicle never collides with itself.
pub trait CollisionQuery {
    fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Hit>;
    fn raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        rays.iter()
            .map(|r| self.raycast(r.origin, r.direction, r.distance))
            .collect()
    }
    /// Vehicle/pedestrian contacts can differ from camera/world contacts.
    fn actor_raycast(&self, origin: Vec3, direction: Vec3, distance: f32) -> Option<Hit> {
        self.raycast(origin, direction, distance)
    }
    fn actor_raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        self.raycast_batch(rays)
    }
}

pub struct ActorQuery<'a, Q: CollisionQuery + ?Sized>(pub &'a Q);
impl<Q: CollisionQuery + ?Sized> CollisionQuery for ActorQuery<'_, Q> {
    fn raycast(&self, o: Vec3, d: Vec3, distance: f32) -> Option<Hit> {
        self.0.actor_raycast(o, d, distance)
    }
    fn raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        self.0.actor_raycast_batch(rays)
    }
}

#[derive(Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub distance: f32,
}

#[derive(Clone)]
pub struct NativeCollider {
    triangles: Vec<Triangle>,
    spheres: Vec<Sphere>,
    bounds: [Vec3; 2],
}

impl NativeCollider {
    /// Use the original shared three-sphere pedestrian body, with the declared
    /// bottom of CColModel aligned to the controller's feet. This body remains
    /// upright; it is not the separate animated hit model used for combat.
    pub fn load_pedestrian(game: &[u8], model_id: u16) -> Result<Self> {
        let count = u32le(game, 0x38)? as usize;
        if count > MAX_MODELS || model_id as usize >= count {
            return Err("Invalid pedestrian collision model ID".into());
        }
        let table = checked_ptr(game, 0x3c, count * 4)?;
        let info = checked_ptr(game, table + model_id as usize * 4, 0x18)?;
        let address = checked_ptr(game, info + 0x14, 0x60)?;
        let bottom = f32le(game, address + 0x18)?;
        if !bottom.is_finite() || bottom.abs() > 10.0 {
            return Err("Invalid pedestrian collision base".into());
        }
        let root = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -bottom, 1.0,
        ];
        Self::from_model(&decode_model(game, address)?, model_id, &root)
    }

    /// Vehicle CColModel coordinates are relative to its clump root. Apply the
    /// SAME root matrix as the visible mesh (Banshee root is translated +0.7 Z).
    pub fn load_embedded(game: &[u8], model_id: u16, root: &[f32; 16]) -> Result<Self> {
        let count = u32le(game, 0x38)? as usize;
        if count > MAX_MODELS || model_id as usize >= count {
            return Err("Invalid dynamic collision model ID".into());
        }
        let table = checked_ptr(game, 0x3c, count * 4)?;
        let info = checked_ptr(game, table + model_id as usize * 4, 0x18)?;
        let address = checked_ptr(game, info + 0x14, 0x60)?;
        let model = decode_model(game, address)?;
        Self::from_model(&model, model_id, root)
    }

    fn from_model(model: &Model, model_id: u16, root: &[f32; 16]) -> Result<Self> {
        let inverse = inverse_linear(root)?;
        let mut triangles: Vec<_> = model
            .triangles
            .iter()
            .map(|(vertices, surface)| Triangle {
                vertices: vertices.map(|p| transform(root, p)),
                surface: *surface,
                model_id,
            })
            .collect();
        for (bounds, surface) in &model.boxes {
            let points: [Vec3; 8] = std::array::from_fn(|i| {
                transform(
                    root,
                    [
                        bounds[i & 1][0],
                        bounds[(i >> 1) & 1][1],
                        bounds[(i >> 2) & 1][2],
                    ],
                )
            });
            for f in [
                [0, 1, 3],
                [0, 3, 2],
                [4, 6, 7],
                [4, 7, 5],
                [0, 4, 5],
                [0, 5, 1],
                [2, 3, 7],
                [2, 7, 6],
                [0, 2, 6],
                [0, 6, 4],
                [1, 5, 7],
                [1, 7, 3],
            ] {
                triangles.push(Triangle {
                    vertices: f.map(|i| points[i]),
                    surface: *surface,
                    model_id,
                });
            }
        }
        let spheres = model
            .spheres
            .iter()
            .map(|(center, radius, surface)| {
                let wc = transform(root, *center);
                let mut bounds = [wc, wc];
                for a in 0..3 {
                    let extent = radius
                        * (root[a].powi(2) + root[4 + a].powi(2) + root[8 + a].powi(2)).sqrt();
                    bounds[0][a] -= extent;
                    bounds[1][a] += extent;
                }
                Sphere {
                    center: *center,
                    radius: *radius,
                    inverse,
                    origin: [root[12], root[13], root[14]],
                    bounds,
                    surface: *surface,
                    model_id,
                    level: 0,
                    entity: None,
                }
            })
            .collect();
        Ok(Self::from_primitives(triangles, spheres))
    }

    fn from_primitives(triangles: Vec<Triangle>, spheres: Vec<Sphere>) -> Self {
        let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
        for p in triangles
            .iter()
            .flat_map(|t| t.vertices)
            .chain(spheres.iter().flat_map(|s| s.bounds))
        {
            for a in 0..3 {
                bounds[0][a] = bounds[0][a].min(p[a]);
                bounds[1][a] = bounds[1][a].max(p[a]);
            }
        }
        Self {
            triangles,
            spheres,
            bounds,
        }
    }

    pub fn primitive_counts(&self) -> [usize; 2] {
        [self.triangles.len(), self.spheres.len()]
    }

    pub fn at(&self, position: Vec3, heading: f32) -> CollisionInstance<'_> {
        let (s, c) = heading.sin_cos();
        self.transformed(&[
            c,
            -s,
            0.,
            0.,
            s,
            c,
            0.,
            0.,
            0.,
            0.,
            1.,
            0.,
            position[0],
            position[1],
            position[2],
            1.,
        ])
    }

    /// Rigid world-space transform; callers supply an orthonormal basis.
    pub fn transformed(&self, matrix: &[f32; 16]) -> CollisionInstance<'_> {
        CollisionInstance {
            collider: self,
            matrix: *matrix,
        }
    }

    pub fn support_points(&self) -> Vec<Vec3> {
        let mut points: Vec<_> = self.triangles.iter().flat_map(|t| t.vertices).collect();
        for s in &self.spheres {
            // Bounds conservatively cover the serialized ellipsoid.
            for i in 0..8 {
                points.push(std::array::from_fn(|a| s.bounds[(i >> a) & 1][a]));
            }
        }
        points.sort_by(|a, b| {
            a[0].total_cmp(&b[0])
                .then(a[1].total_cmp(&b[1]))
                .then(a[2].total_cmp(&b[2]))
        });
        points.dedup();
        points
    }
}

#[derive(Clone, Copy)]
pub struct CollisionInstance<'a> {
    collider: &'a NativeCollider,
    matrix: [f32; 16],
}

impl CollisionInstance<'_> {
    /// Conservative occupancy guard for restoring distant native placements.
    /// Test in the actor's local space; this can defer restoration around a
    /// hollow model but cannot restore furniture through a nearby actor.
    fn overlaps_sphere(&self, center: Vec3, radius: f32) -> bool {
        let d = sub(center, self.matrix[12..15].try_into().unwrap());
        let p: Vec3 = std::array::from_fn(|a| (0..3).map(|i| self.matrix[a * 4 + i] * d[i]).sum());
        (0..3)
            .map(|a| {
                let delta =
                    p[a] - p[a].clamp(self.collider.bounds[0][a], self.collider.bounds[1][a]);
                delta * delta
            })
            .sum::<f32>()
            <= radius * radius
    }
}
impl CollisionQuery for CollisionInstance<'_> {
    fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Hit> {
        if origin.iter().any(|v| !v.is_finite()) || !max_distance.is_finite() || max_distance <= 0.0
        {
            return None;
        }
        let direction = normalized(direction)?;
        let m = &self.matrix;
        let local = |v: Vec3| {
            std::array::from_fn(|a| m[a * 4] * v[0] + m[a * 4 + 1] * v[1] + m[a * 4 + 2] * v[2])
        };
        let world =
            |v: Vec3| std::array::from_fn(|a| m[a] * v[0] + m[4 + a] * v[1] + m[8 + a] * v[2]);
        let o = local(sub(origin, [m[12], m[13], m[14]]));
        let d = local(direction);
        if !ray_bounds(o, d, max_distance, self.collider.bounds) {
            return None;
        }
        let mut nearest = max_distance;
        let mut hit = None;
        for tri in &self.collider.triangles {
            if let Some(contact) = ray_triangle(o, d, nearest, tri) {
                nearest = contact.distance;
                hit = Some(contact);
            }
        }
        for sphere in &self.collider.spheres {
            if let Some(contact) = ray_sphere(o, d, nearest, sphere) {
                nearest = contact.distance;
                hit = Some(contact);
            }
        }
        hit.map(|mut contact| {
            contact.position = transform(m, contact.position);
            contact.normal = world(contact.normal);
            contact
        })
    }
}

pub struct CollisionScene<'a> {
    pub world: &'a dyn CollisionQuery,
    pub instances: &'a [CollisionInstance<'a>],
}
impl CollisionQuery for CollisionScene<'_> {
    fn actor_raycast(&self, o: Vec3, d: Vec3, distance: f32) -> Option<Hit> {
        CollisionScene {
            world: &ActorQuery(self.world),
            instances: self.instances,
        }
        .raycast(o, d, distance)
    }
    fn actor_raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        CollisionScene {
            world: &ActorQuery(self.world),
            instances: self.instances,
        }
        .raycast_batch(rays)
    }
    fn raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        let mut hits = self.world.raycast_batch(rays);
        for instance in self.instances {
            for (r, hit) in rays.iter().zip(&mut hits) {
                if let Some(h) = instance.raycast(
                    r.origin,
                    r.direction,
                    hit.as_ref().map_or(r.distance, |h| h.distance),
                ) {
                    *hit = Some(h);
                }
            }
        }
        hits
    }
    fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Hit> {
        let mut hit = self.world.raycast(origin, direction, max_distance);
        for instance in self.instances {
            let max = hit.as_ref().map_or(max_distance, |h| h.distance);
            if let Some(contact) = instance.raycast(origin, direction, max) {
                hit = Some(contact);
            }
        }
        hit
    }
}

impl CollisionQuery for CollisionWorld {
    fn actor_raycast(&self, o: Vec3, d: Vec3, distance: f32) -> Option<Hit> {
        self.query(o, d, distance, None, true)
    }
    fn actor_raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        self.batch(rays, true)
    }
    fn raycast_batch(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        self.batch(rays, false)
    }
    fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Hit> {
        CollisionWorld::raycast(self, origin, direction, max_distance)
    }
}

#[cfg(test)]
pub(crate) fn test_box_collider(bounds: [Vec3; 2]) -> NativeCollider {
    let model = Model {
        triangles: Vec::new(),
        boxes: vec![(bounds, 1)],
        spheres: Vec::new(),
    };
    NativeCollider::from_model(
        &model,
        175,
        &[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
    )
    .unwrap()
}

#[derive(Default, Serialize, Debug)]
pub struct CollisionStats {
    pub embedded_models: usize,
    pub streamed_chunks: usize,
    pub streamed_models: usize,
    pub decoded_models: usize,
    pub allocated_entities: usize,
    pub collidable_entities: usize,
    pub empty_entities: usize,
    pub missing_models: usize,
    pub triangles: usize,
    pub original_boxes: usize,
    pub original_spheres: usize,
    pub level_entities: HashMap<u8, usize>,
}

pub struct CollisionWorld {
    pub stats: CollisionStats,
    pub triangles: Vec<Triangle>,
    spheres: Vec<Sphere>,
    cells: HashMap<[i32; 2], Vec<usize>>,
    oversized: Vec<usize>,
    triangle_levels: Vec<u8>,
    active_levels: Option<Vec<u8>>,
    triangle_entities: Vec<Option<usize>>,
    entities: HashMap<usize, EntityRange>,
    moved: HashMap<usize, MovingCollider>,
}

struct EntityRange {
    triangles: std::ops::Range<usize>,
    spheres: std::ops::Range<usize>,
    level: u8,
}
struct MovingCollider {
    shape: NativeCollider,
    matrix: [f32; 16],
    bounds: [Vec3; 2],
    points: Vec<Vec3>,
    level: u8,
    ignore_actors: bool,
}

#[derive(Clone)]
struct Model {
    triangles: Vec<([Vec3; 3], u8)>,
    boxes: Vec<([Vec3; 2], u8)>,
    spheres: Vec<(Vec3, f32, u8)>,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalized(a: Vec3) -> Option<Vec3> {
    let n = dot(a, a).sqrt();
    if !n.is_finite() || n < 1e-12 {
        None
    } else {
        Some([a[0] / n, a[1] / n, a[2] / n])
    }
}
fn transform(m: &[f32; 16], p: Vec3) -> Vec3 {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}
fn inverse_linear(m: &[f32; 16]) -> Result<[[f32; 3]; 3]> {
    let a = [m[0], m[1], m[2]];
    let b = [m[4], m[5], m[6]];
    let c = [m[8], m[9], m[10]];
    let det = dot(a, cross(b, c));
    if !det.is_finite() || det.abs() < 1e-8 {
        return Err("Singular collision placement matrix".into());
    }
    let mut inv = [cross(b, c), cross(c, a), cross(a, b)];
    for r in &mut inv {
        for v in r {
            *v /= det;
        }
    }
    Ok(inv)
}
fn inv_transform(m: &[[f32; 3]; 3], v: Vec3) -> Vec3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
fn read_vector(data: &[u8], off: usize) -> Result<Vec3> {
    let v = [
        f32le(data, off)?,
        f32le(data, off + 4)?,
        f32le(data, off + 8)?,
    ];
    if v.iter().any(|n| !n.is_finite() || n.abs() > 1.0e7) {
        return Err("Invalid collision vector".into());
    }
    Ok(v)
}
fn checked_ptr(data: &[u8], field: usize, size: usize) -> Result<usize> {
    let p = u32le(data, field)? as usize;
    if p < 32 && size > 0 {
        return Err(format!("Missing collision pointer at {field:#x}").into());
    }
    if size > 0 {
        bytes(data, p, size)?;
    }
    Ok(p)
}

fn decode_model(data: &[u8], off: usize) -> Result<Model> {
    bytes(data, off, 0x60)?;
    let nb = u16le(data, off + 0x30)? as usize;
    let nt = u16le(data, off + 0x32)? as usize;
    let ns = u16le(data, off + 0x34)? as usize;
    if nb > 32767 || nt > 32767 || ns > 32767 {
        return Err("Negative collision model count".into());
    }
    let bp = checked_ptr(data, off + 0x40, nb * 48)?;
    let sp = checked_ptr(data, off + 0x38, ns * 32)?;
    let tp = checked_ptr(data, off + 0x4c, nt * 8)?;
    let mut faces = Vec::with_capacity(nt);
    let mut max_vertex = 0;
    for i in 0..nt {
        let row = tp + i * 8;
        let mut idx = [0; 3];
        for j in 0..3 {
            let n = u16le(data, row + j * 2)? as usize;
            // These are byte offsets into the six-byte vertex array, not indices.
            if n % 6 != 0 {
                return Err("Unaligned compressed collision vertex offset".into());
            }
            idx[j] = n / 6;
            max_vertex = max_vertex.max(idx[j]);
        }
        faces.push((idx, data[row + 6]));
    }
    let vp = checked_ptr(
        data,
        off + 0x48,
        if nt > 0 { (max_vertex + 1) * 6 } else { 0 },
    )?;
    let mut vertices = Vec::with_capacity(if nt > 0 { max_vertex + 1 } else { 0 });
    if nt > 0 {
        for i in 0..=max_vertex {
            let p = vp + i * 6;
            vertices.push([
                (u16le(data, p)? as i16) as f32 / 128.0,
                (u16le(data, p + 2)? as i16) as f32 / 128.0,
                (u16le(data, p + 4)? as i16) as f32 / 128.0,
            ]);
        }
    }
    let triangles = faces
        .into_iter()
        .map(|(i, s)| ([vertices[i[0]], vertices[i[1]], vertices[i[2]]], s))
        .collect();
    let mut boxes = Vec::with_capacity(nb);
    for i in 0..nb {
        let p = bp + i * 48;
        let mut lo = read_vector(data, p)?;
        let mut hi = read_vector(data, p + 16)?;
        for a in 0..3 {
            if lo[a] > hi[a] {
                std::mem::swap(&mut lo[a], &mut hi[a]);
            }
        }
        boxes.push(([lo, hi], data[p + 32]));
    }
    let mut spheres = Vec::with_capacity(ns);
    for i in 0..ns {
        let p = sp + i * 32;
        let center = read_vector(data, p)?;
        let radius = f32le(data, p + 12)?;
        if !radius.is_finite() || radius < 0.0 || radius > 1.0e6 {
            return Err("Invalid collision sphere radius".into());
        }
        spheres.push((center, radius, data[p + 16]));
    }
    Ok(Model {
        triangles,
        boxes,
        spheres,
    })
}

fn validate_chunk<'a>(data: &'a [u8], magic: &[u8]) -> Result<&'a [u8]> {
    if bytes(data, 0, 4)? != magic {
        return Err("Unexpected collision/resource chunk signature".into());
    }
    let size = u32le(data, 8)? as usize;
    let body = u32le(data, 12)? as usize;
    let reloc = u32le(data, 16)? as usize;
    let count = u32le(data, 20)? as usize;
    if size < 32 || size > data.len() || body < 32 || body > size || count > MAX_POOL * 32 {
        return Err("Invalid collision/resource chunk size".into());
    }
    bytes(data, reloc, count * 4)?;
    // Pointers are offsets relative to the header. Do not apply host-address fixups.
    for i in 0..count {
        let field = u32le(data, reloc + i * 4)? as usize;
        // GAME.DTZ also relocates class metadata between dataSize and relocTab.
        if field < 32 || field + 4 > reloc {
            return Err("Invalid collision relocation field".into());
        }
        let target = u32le(data, field)? as usize;
        if target > reloc {
            return Err("Invalid collision relocation target".into());
        }
    }
    Ok(&data[..size])
}

/// Loads static BUILDING, TREADABLE and DUMMY placements from inflated GAME.DTZ.
/// Streamed COL2 models replace embedded bootstrap models just as in the game.
/// The result contains all serialized levels; spatial queries use world positions.
/// Dynamic vehicles/peds and runtime object state are outside this static reader.
pub fn load_world(disc: &mut Disc, game: &[u8]) -> Result<CollisionWorld> {
    let game = validate_chunk(game, b"GATG")?;
    let count = u32le(game, 0x38)? as usize;
    if count > MAX_MODELS {
        return Err("Invalid collision model info count".into());
    }
    let table = checked_ptr(game, 0x3c, count * 4)?;
    let mut stats = CollisionStats::default();
    let mut models = HashMap::<u16, Model>::new();
    let mut decoded = HashMap::<usize, Model>::new();
    for id in 0..count {
        let p = u32le(game, table + id * 4)? as usize;
        if p == 0 {
            continue;
        }
        bytes(game, p, 40)?;
        let cm = u32le(game, p + 0x14)? as usize;
        if cm == 0 {
            continue;
        }
        if !decoded.contains_key(&cm) {
            decoded.insert(cm, decode_model(game, cm)?);
        }
        models.insert(id as u16, decoded[&cm].clone());
        stats.embedded_models += 1;
    }
    let streaming = checked_ptr(game, 0x7c, 0xd0)?;
    let first = u32le(game, streaming + 12)? as usize;
    let end = u32le(game, streaming + 16)? as usize;
    let stream_count = u32le(game, streaming + 20)? as usize;
    if first > end || end > stream_count || stream_count > MAX_MODELS * 2 {
        return Err("Invalid streamed collision range".into());
    }
    let stream_table = checked_ptr(game, streaming + 0xc8, stream_count * 24)?;
    for id in first..end {
        let row = stream_table + id * 24;
        let sector = u32le(game, row + 16)? as usize;
        let sectors = u32le(game, row + 20)? as usize;
        if sectors == 0 {
            continue;
        }
        if sectors > 65536 {
            return Err("Streamed collision extent exceeds 128 MiB".into());
        }
        let chunk = disc.range(
            "GTA3PS2.IMG",
            sector
                .checked_mul(2048)
                .ok_or("Collision extent overflow")?,
            sectors * 2048,
        )?;
        let chunk = validate_chunk(&chunk, b"2loc")?;
        stats.streamed_chunks += 1;
        let mut off = 32;
        let mut terminated = false;
        while off + 8 <= chunk.len() {
            let mid = u32le(chunk, off)? as i32;
            if mid < 0 {
                terminated = true;
                break;
            }
            if mid as usize >= count {
                return Err("Streamed collision model ID outside model table".into());
            }
            let cm = checked_ptr(chunk, off + 4, 0x60)?;
            models.insert(mid as u16, decode_model(chunk, cm)?);
            stats.streamed_models += 1;
            off += 8;
            if off > 32 + count * 8 {
                return Err("Unterminated streamed collision model table".into());
            }
        }
        if !terminated {
            return Err("Missing streamed collision table terminator".into());
        }
    }
    stats.decoded_models = models.len();
    let mut world = CollisionWorld {
        stats,
        triangles: Vec::new(),
        spheres: Vec::new(),
        cells: HashMap::new(),
        oversized: Vec::new(),
        triangle_levels: Vec::new(),
        active_levels: None,
        triangle_entities: Vec::new(),
        entities: HashMap::new(),
        moved: HashMap::new(),
    };
    for header in [0x24, 0x28, 0x2c] {
        let pool = checked_ptr(game, header, 32)?;
        let n = u32le(game, pool + 8)? as usize;
        if n > MAX_POOL {
            return Err("Invalid collision entity pool size".into());
        }
        if n == 0 {
            continue;
        } // Empty PS2 pools retain nonserialized runtime pointers.
        let items = checked_ptr(game, pool, n * 0x60)?;
        let flags = checked_ptr(game, pool + 4, n)?;
        for i in 0..n {
            if game[flags + i] & 0x80 != 0 {
                continue;
            }
            world.stats.allocated_entities += 1;
            let row = items + i * 0x60;
            let mid = u16le(game, row + 0x56)?;
            let level = game[row + 0x5a];
            *world
                .stats
                .level_entities
                .entry(game[row + 0x5a])
                .or_default() += 1;
            let Some(model) = models.get(&mid) else {
                world.stats.missing_models += 1;
                continue;
            };
            if model.triangles.is_empty() && model.boxes.is_empty() && model.spheres.is_empty() {
                world.stats.empty_entities += 1;
                continue;
            }
            let mut matrix = [0.0; 16];
            for c in 0..4 {
                let v = read_vector(game, row + c * 16)?;
                matrix[c * 4..c * 4 + 3].copy_from_slice(&v);
            }
            // Matrix fourth components are runtime flags, not float coordinates.
            let triangle_start = world.triangles.len();
            let sphere_start = world.spheres.len();
            let inverse = inverse_linear(&matrix)?;
            world.stats.collidable_entities += 1;
            for (vertices, surface) in &model.triangles {
                world.triangles.push(Triangle {
                    vertices: vertices.map(|p| transform(&matrix, p)),
                    surface: *surface,
                    model_id: mid,
                });
                world.triangle_levels.push(level);
            }
            for (b, surface) in &model.boxes {
                world.stats.original_boxes += 1;
                let mut points = [[0.0; 3]; 8];
                for j in 0..8 {
                    points[j] = transform(
                        &matrix,
                        [
                            b[(j & 1) as usize][0],
                            b[((j >> 1) & 1) as usize][1],
                            b[((j >> 2) & 1) as usize][2],
                        ],
                    );
                }
                for f in [
                    [0, 1, 3],
                    [0, 3, 2],
                    [4, 6, 7],
                    [4, 7, 5],
                    [0, 4, 5],
                    [0, 5, 1],
                    [2, 3, 7],
                    [2, 7, 6],
                    [0, 2, 6],
                    [0, 6, 4],
                    [1, 5, 7],
                    [1, 7, 3],
                ] {
                    world.triangles.push(Triangle {
                        vertices: [points[f[0]], points[f[1]], points[f[2]]],
                        surface: *surface,
                        model_id: mid,
                    });
                    world.triangle_levels.push(level);
                }
            }
            for (center, radius, surface) in &model.spheres {
                world.stats.original_spheres += 1;
                let wc = transform(&matrix, *center);
                let mut bounds = [wc, wc];
                for a in 0..3 {
                    let extent = radius
                        * (matrix[a] * matrix[a]
                            + matrix[4 + a] * matrix[4 + a]
                            + matrix[8 + a] * matrix[8 + a])
                            .sqrt();
                    bounds[0][a] -= extent;
                    bounds[1][a] += extent;
                }
                world.spheres.push(Sphere {
                    center: *center,
                    radius: *radius,
                    inverse,
                    origin: [matrix[12], matrix[13], matrix[14]],
                    bounds,
                    surface: *surface,
                    model_id: mid,
                    level,
                    entity: Some(row),
                });
            }
            world
                .triangle_entities
                .resize(world.triangles.len(), Some(row));
            world.entities.insert(
                row,
                EntityRange {
                    triangles: triangle_start..world.triangles.len(),
                    spheres: sphere_start..world.spheres.len(),
                    level,
                },
            );
        }
    }
    world.stats.triangles = world.triangles.len();
    world.build_index();
    Ok(world)
}

fn triangle_bounds(t: &Triangle) -> [Vec3; 2] {
    let mut bounds = [t.vertices[0], t.vertices[0]];
    for p in &t.vertices[1..] {
        for a in 0..3 {
            bounds[0][a] = bounds[0][a].min(p[a]);
            bounds[1][a] = bounds[1][a].max(p[a]);
        }
    }
    bounds
}
fn cell(p: Vec3) -> [i32; 2] {
    [(p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32]
}

impl CollisionWorld {
    #[cfg(test)]
    pub(crate) fn test_add_entity(&mut self, entity: usize, shape: NativeCollider, level: u8) {
        let triangles = self.triangles.len()..self.triangles.len() + shape.triangles.len();
        let spheres = self.spheres.len()..self.spheres.len() + shape.spheres.len();
        self.triangles.extend(shape.triangles);
        self.triangle_entities
            .resize(self.triangles.len(), Some(entity));
        self.triangle_levels.resize(self.triangles.len(), level);
        self.spheres.extend(shape.spheres.into_iter().map(|mut s| {
            s.entity = Some(entity);
            s.level = level;
            s
        }));
        self.entities.insert(
            entity,
            EntityRange {
                triangles,
                spheres,
                level,
            },
        );
        self.cells.clear();
        self.oversized.clear();
        self.build_index();
    }
    /// Builds an indexed world from known collision triangles (tools/test scenes).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_triangles(triangles: Vec<Triangle>) -> Self {
        let triangle_count = triangles.len();
        let stats = CollisionStats {
            triangles: triangle_count,
            ..CollisionStats::default()
        };
        let mut world = Self {
            stats,
            triangles,
            spheres: Vec::new(),
            cells: HashMap::new(),
            oversized: Vec::new(),
            triangle_levels: vec![0; triangle_count],
            active_levels: None,
            triangle_entities: Vec::new(),
            entities: HashMap::new(),
            moved: HashMap::new(),
        };
        world.triangle_entities.resize(triangle_count, None);
        world.build_index();
        world
    }

    /// Selects serialized map level 1=BEACH, 2=MAINLA, 3=MALL. Level 0 remains
    /// global. `None` queries all native placements, including level250 dummy
    /// objects whose runtime activation has not yet been reverse engineered.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_active_level(&mut self, level: Option<u8>) {
        self.active_levels = level.map(|level| vec![level]);
    }
    /// Query multiple exterior zones together while retaining global colliders.
    pub fn set_active_levels(&mut self, levels: &[u8]) {
        self.active_levels = Some(levels.to_vec());
    }

    pub fn entity_active(&self, entity: usize) -> bool {
        self.entities.get(&entity).is_some_and(|e| {
            e.level == 0
                || self
                    .active_levels
                    .as_ref()
                    .is_none_or(|a| a.contains(&e.level))
        })
    }

    /// Coherent car probes share a small swept box. Select original primitives
    /// once for that box, instead of scanning an entire 32m cell for every ray.
    /// This only accelerates queries; contacts still use the original geometry.
    fn batch(&self, rays: &[Ray], actor: bool) -> Vec<Option<Hit>> {
        let normalized_rays: Vec<_> = rays
            .iter()
            .map(|r| {
                if r.origin.iter().any(|v| !v.is_finite())
                    || !r.distance.is_finite()
                    || r.distance <= 0.
                {
                    None
                } else {
                    normalized(r.direction).map(|d| (r.origin, d, r.distance))
                }
            })
            .collect();
        let mut bounds = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
        for &(o, d, length) in normalized_rays.iter().flatten() {
            for a in 0..3 {
                let end = o[a] + d[a] * length;
                bounds[0][a] = bounds[0][a].min(o[a].min(end));
                bounds[1][a] = bounds[1][a].max(o[a].max(end));
            }
        }
        if !bounds[0][0].is_finite() {
            return vec![None; rays.len()];
        }
        let lo = cell(bounds[0]);
        let hi = cell(bounds[1]);
        let size =
            (hi[0] as i64 - lo[0] as i64 + 1).saturating_mul(hi[1] as i64 - lo[1] as i64 + 1);
        if size > 4096 {
            return rays
                .iter()
                .map(|r| self.query(r.origin, r.direction, r.distance, None, actor))
                .collect();
        }
        let overlaps = |b: [Vec3; 2]| {
            (0..3).all(|a| b[0][a] <= bounds[1][a] + 0.0001 && b[1][a] >= bounds[0][a] - 0.0001)
        };
        let mut ids: HashSet<_> = self.oversized.iter().copied().collect();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                if let Some(cell) = self.cells.get(&[x, y]) {
                    ids.extend(cell);
                }
            }
        }
        let candidates: Vec<_> = ids
            .into_iter()
            .filter_map(|id| {
                let (level, entity, b) = if id < self.triangles.len() {
                    (
                        self.triangle_levels[id],
                        self.triangle_entities[id],
                        triangle_bounds(&self.triangles[id]),
                    )
                } else {
                    let s = &self.spheres[id - self.triangles.len()];
                    (s.level, s.entity, s.bounds)
                };
                if self
                    .active_levels
                    .as_ref()
                    .is_some_and(|a| level != 0 && !a.contains(&level))
                    || entity.is_some_and(|e| self.moved.contains_key(&e))
                    || !overlaps(b)
                {
                    None
                } else {
                    Some((id, entity, b))
                }
            })
            .collect();
        let moved: Vec<_> = self
            .moved
            .iter()
            .filter(|(e, b)| {
                self.entity_active(**e) && !(actor && b.ignore_actors) && overlaps(b.bounds)
            })
            .collect();
        normalized_rays
            .into_iter()
            .map(|r| {
                let (o, d, mut nearest) = r?;
                let mut hit = None;
                for &(id, entity, b) in &candidates {
                    if !ray_bounds(o, d, nearest, b) {
                        continue;
                    }
                    let candidate = if id < self.triangles.len() {
                        ray_triangle(o, d, nearest, &self.triangles[id])
                    } else {
                        ray_sphere(o, d, nearest, &self.spheres[id - self.triangles.len()])
                    };
                    if let Some(mut h) = candidate {
                        h.entity = entity;
                        nearest = h.distance;
                        hit = Some(h);
                    }
                }
                for &(entity, body) in &moved {
                    if !ray_bounds(o, d, nearest, body.bounds) {
                        continue;
                    }
                    if let Some(mut h) = body.shape.transformed(&body.matrix).raycast(o, d, nearest)
                    {
                        h.entity = Some(*entity);
                        nearest = h.distance;
                        hit = Some(h);
                    }
                }
                hit
            })
            .collect()
    }

    fn build_index(&mut self) {
        for id in 0..self.triangles.len() + self.spheres.len() {
            let bounds = if id < self.triangles.len() {
                triangle_bounds(&self.triangles[id])
            } else {
                self.spheres[id - self.triangles.len()].bounds
            };
            let lo = cell(bounds[0]);
            let hi = cell(bounds[1]);
            let count = (hi[0] as i64 - lo[0] as i64 + 1) * (hi[1] as i64 - lo[1] as i64 + 1);
            if count > 4096 {
                self.oversized.push(id);
                continue;
            }
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    self.cells.entry([x, y]).or_default().push(id);
                }
            }
        }
    }

    /// Two-sided raycast against original static PS2 collision surfaces.
    /// `direction` may be any nonzero finite vector and is normalized internally.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<Hit> {
        self.raycast_except(origin, direction, max_distance, None)
    }

    /// Excludes one awake prop from its own terrain/obstacle queries.
    pub fn raycast_except(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        skip: Option<usize>,
    ) -> Option<Hit> {
        self.query(origin, direction, max_distance, skip, false)
    }

    fn query(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        skip: Option<usize>,
        actor: bool,
    ) -> Option<Hit> {
        if origin.iter().any(|v| !v.is_finite()) || !max_distance.is_finite() || max_distance <= 0.0
        {
            return None;
        }
        let direction = normalized(direction)?;
        let endpoint = [
            origin[0] + direction[0] * max_distance,
            origin[1] + direction[1] * max_distance,
            origin[2] + direction[2] * max_distance,
        ];
        let mut current = cell(origin);
        let end = cell(endpoint);
        let mut step = [0; 2];
        let mut next = [f32::INFINITY; 2];
        let mut delta = [f32::INFINITY; 2];
        for a in 0..2 {
            if direction[a].abs() > 1e-12 {
                step[a] = if direction[a] > 0.0 { 1 } else { -1 };
                let boundary = (current[a] + if step[a] > 0 { 1 } else { 0 }) as f32 * CELL;
                next[a] = ((boundary - origin[a]) / direction[a]).max(0.0);
                delta[a] = CELL / direction[a].abs();
            }
        }
        let mut candidates = HashSet::new();
        candidates.extend(self.oversized.iter().copied());
        // Traverse only cells crossed by the ray, avoiding a giant rectangular scan.
        // Finite game coordinates/rays have bounded work; queries beyond 200k cells
        // are outside the supported static-map scale.
        for _ in 0..200000 {
            if let Some(ids) = self.cells.get(&current) {
                candidates.extend(ids.iter().copied());
            }
            if current == end {
                break;
            }
            let a = if next[0] <= next[1] { 0 } else { 1 };
            if next[a] > max_distance {
                break;
            }
            current[a] = current[a].checked_add(step[a])?;
            next[a] += delta[a];
        }
        let mut nearest = max_distance;
        let mut hit = None;
        for id in candidates {
            let entity = if id < self.triangles.len() {
                self.triangle_entities[id]
            } else {
                self.spheres[id - self.triangles.len()].entity
            };
            if entity.is_some_and(|e| self.moved.contains_key(&e) || skip == Some(e)) {
                continue;
            }
            let level = if id < self.triangles.len() {
                self.triangle_levels[id]
            } else {
                self.spheres[id - self.triangles.len()].level
            };
            if let Some(active) = &self.active_levels {
                if level != 0 && !active.contains(&level) {
                    continue;
                }
            }
            let candidate = if id < self.triangles.len() {
                ray_triangle(origin, direction, nearest, &self.triangles[id])
            } else {
                ray_sphere(
                    origin,
                    direction,
                    nearest,
                    &self.spheres[id - self.triangles.len()],
                )
            };
            if let Some(mut c) = candidate {
                c.entity = entity;
                nearest = c.distance;
                hit = Some(c);
            }
        }
        for (&entity, body) in &self.moved {
            if skip == Some(entity)
                || (actor && body.ignore_actors)
                || self
                    .active_levels
                    .as_ref()
                    .is_some_and(|a| body.level != 0 && !a.contains(&body.level))
            {
                continue;
            }
            if !ray_bounds(origin, direction, nearest, body.bounds) {
                continue;
            }
            if let Some(mut c) = body
                .shape
                .transformed(&body.matrix)
                .raycast(origin, direction, nearest)
            {
                c.entity = Some(entity);
                nearest = c.distance;
                hit = Some(c);
            }
        }
        hit
    }

    /// Retain indexed static geometry until the exact placement is awakened.
    pub fn awaken(&mut self, entity: usize) -> Option<Vec<Vec3>> {
        if let Some(body) = self.moved.get(&entity) {
            return Some(body.points.clone());
        }
        let range = self.entities.get(&entity)?;
        let shape = NativeCollider::from_primitives(
            self.triangles[range.triangles.clone()].to_vec(),
            self.spheres[range.spheres.clone()].to_vec(),
        );
        let points = shape.support_points();
        if points.is_empty() {
            return None;
        }
        let matrix = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        self.moved.insert(
            entity,
            MovingCollider {
                shape,
                matrix,
                bounds: point_bounds(&points),
                points: points.clone(),
                level: range.level,
                ignore_actors: false,
            },
        );
        Some(points)
    }

    pub fn move_entity(&mut self, entity: usize, matrix: [f32; 16]) {
        if let Some(body) = self.moved.get_mut(&entity) {
            body.matrix = matrix;
            body.bounds = point_bounds(
                &body
                    .points
                    .iter()
                    .map(|&p| transform(&matrix, p))
                    .collect::<Vec<_>>(),
            );
        }
    }

    pub fn set_ignores_actors(&mut self, entity: usize, ignore: bool) {
        if let Some(body) = self.moved.get_mut(&entity) {
            body.ignore_actors = ignore;
        }
    }

    /// Restore the original indexed shape only when its visible instance is
    /// also restored. Removing an awake shape alone would leave a ghost pole.
    pub fn restore_entity(&mut self, entity: usize) {
        self.moved.remove(&entity);
    }

    pub fn restoration_clear(
        &self,
        entity: usize,
        origin: Vec3,
        radius: f32,
        actors: &[CollisionInstance<'_>],
    ) -> bool {
        !actors.iter().any(|a| a.overlaps_sphere(origin, radius))
            && !self.moved.iter().any(|(&id, b)| {
                id != entity
                    && self.entity_active(id)
                    && b.shape
                        .transformed(&b.matrix)
                        .overlaps_sphere(origin, radius)
            })
    }
}

fn point_bounds(points: &[Vec3]) -> [Vec3; 2] {
    let mut bounds = [points[0]; 2];
    for p in points {
        for a in 0..3 {
            bounds[0][a] = bounds[0][a].min(p[a]);
            bounds[1][a] = bounds[1][a].max(p[a]);
        }
    }
    bounds
}
fn ray_bounds(origin: Vec3, direction: Vec3, distance: f32, bounds: [Vec3; 2]) -> bool {
    let mut near: f32 = 0.;
    let mut far = distance;
    for a in 0..3 {
        if direction[a].abs() < 1e-8 {
            if origin[a] < bounds[0][a] || origin[a] > bounds[1][a] {
                return false;
            }
        } else {
            let x = (bounds[0][a] - origin[a]) / direction[a];
            let y = (bounds[1][a] - origin[a]) / direction[a];
            near = near.max(x.min(y));
            far = far.min(x.max(y));
            if near > far {
                return false;
            }
        }
    }
    true
}

fn ray_triangle(origin: Vec3, d: Vec3, max: f32, tri: &Triangle) -> Option<Hit> {
    let [a, b, c] = tri.vertices;
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-7 {
        return None;
    }
    let t = sub(origin, a);
    let inv = 1.0 / det;
    let u = dot(t, p) * inv;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = cross(t, e1);
    let v = dot(d, q) * inv;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let distance = dot(e2, q) * inv;
    if distance < 0.0 || distance > max {
        return None;
    }
    let mut normal = normalized(cross(e1, e2))?;
    if dot(normal, d) > 0.0 {
        for a in &mut normal {
            *a = -*a;
        }
    }
    Some(Hit {
        distance,
        entity: None,
        position: [
            origin[0] + d[0] * distance,
            origin[1] + d[1] * distance,
            origin[2] + d[2] * distance,
        ],
        normal,
        surface: tri.surface,
        model_id: tri.model_id,
    })
}
fn ray_sphere(origin: Vec3, d: Vec3, max: f32, sphere: &Sphere) -> Option<Hit> {
    let o = sub(
        inv_transform(&sphere.inverse, sub(origin, sphere.origin)),
        sphere.center,
    );
    let ld = inv_transform(&sphere.inverse, d);
    let a = dot(ld, ld);
    let b = dot(o, ld);
    let c = dot(o, o) - sphere.radius * sphere.radius;
    let discriminant = b * b - a * c;
    if discriminant < 0.0 || a <= 1e-12 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = (-b - root) / a;
    let far = (-b + root) / a;
    let distance = if near >= 0.0 { near } else { far };
    if distance < 0.0 || distance > max {
        return None;
    }
    let local_normal = [
        o[0] + ld[0] * distance,
        o[1] + ld[1] * distance,
        o[2] + ld[2] * distance,
    ];
    let inverse = sphere.inverse;
    let mut normal = normalized([
        inverse[0][0] * local_normal[0]
            + inverse[1][0] * local_normal[1]
            + inverse[2][0] * local_normal[2],
        inverse[0][1] * local_normal[0]
            + inverse[1][1] * local_normal[1]
            + inverse[2][1] * local_normal[2],
        inverse[0][2] * local_normal[0]
            + inverse[1][2] * local_normal[1]
            + inverse[2][2] * local_normal[2],
    ])?;
    if dot(normal, d) > 0.0 {
        for n in &mut normal {
            *n = -*n;
        }
    }
    Some(Hit {
        distance,
        entity: None,
        position: [
            origin[0] + d[0] * distance,
            origin[1] + d[1] * distance,
            origin[2] + d[2] * distance,
        ],
        normal,
        surface: sphere.surface,
        model_id: sphere.model_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_collider_rotates_ray_and_normal_and_leaves_no_old_instance() {
        let collider = test_box_collider([[-1.0, -2.0, 0.0], [1.0, 2.0, 1.3]]);
        let car = collider.at([10.0, 20.0, 3.0], std::f32::consts::FRAC_PI_2);
        let hit = car
            .raycast([15.0, 20.0, 3.5], [-1.0, 0.0, 0.0], 10.0)
            .unwrap();
        assert!((hit.distance - 3.0).abs() < 1e-5);
        assert!((hit.position[0] - 12.0).abs() < 1e-5);
        assert!((hit.normal[0] - 1.0).abs() < 1e-5);
        assert_eq!(hit.model_id, 175);
        assert!(car
            .raycast([0.0, -5.0, 0.5], [0.0, 1.0, 0.0], 10.0)
            .is_none());
    }

    #[test]
    fn scene_selects_nearest_contact_without_mutating_static_world() {
        let world = CollisionWorld::from_triangles(vec![Triangle {
            vertices: [[-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [0.0, 10.0, 0.0]],
            surface: 0,
            model_id: 5,
        }]);
        let collider = test_box_collider([[-1.0, -2.0, 0.1], [1.0, 2.0, 1.3]]);
        let instances = [collider.at([0.0; 3], 0.0)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let hit = scene
            .raycast([0.0, 0.0, 4.0], [0.0, 0.0, -1.0], 10.0)
            .unwrap();
        assert!((hit.position[2] - 1.3).abs() < 1e-5);
        assert_eq!(hit.model_id, 175);
        assert_eq!(
            world
                .raycast([0.0, 0.0, 4.0], [0.0, 0.0, -1.0], 10.0)
                .unwrap()
                .model_id,
            5
        );
    }

    #[test]
    fn native_root_translation_applies_to_spheres_and_triangles() {
        let model = Model {
            triangles: vec![([[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]], 0)],
            boxes: Vec::new(),
            spheres: vec![([3.0, 0.0, 0.0], 0.5, 0)],
        };
        let root = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.7, 1.0,
        ];
        let collider = NativeCollider::from_model(&model, 175, &root).unwrap();
        let car = collider.at([0.0; 3], 0.0);
        assert!(
            (car.raycast([0.0, 0.0, 3.0], [0.0, 0.0, -1.0], 4.0)
                .unwrap()
                .position[2]
                - 0.7)
                .abs()
                < 1e-5
        );
        assert!(
            (car.raycast([3.0, 0.0, 3.0], [0.0, 0.0, -1.0], 4.0)
                .unwrap()
                .position[2]
                - 1.2)
                .abs()
                < 1e-5
        );
    }
    fn world(triangles: Vec<Triangle>) -> CollisionWorld {
        CollisionWorld::from_triangles(triangles)
    }
    #[test]
    fn ray_finds_nearest_double_sided_ground_across_negative_grid() {
        let t = |z| Triangle {
            vertices: [[-70.0, -30.0, z], [-10.0, -30.0, z], [-40.0, 30.0, z]],
            surface: 4,
            model_id: 9,
        };
        let w = world(vec![t(0.0), t(2.0)]);
        let h = w
            .raycast([-40.0, 0.0, 10.0], [0.0, 0.0, -2.0], 20.0)
            .unwrap();
        assert!((h.distance - 8.0).abs() < 1e-5);
        assert_eq!(h.position, [-40.0, 0.0, 2.0]);
        assert_eq!(h.normal, [0.0, 0.0, 1.0]);
        assert!(w
            .raycast([-40.0, 0.0, 10.0], [0.0, 0.0, 0.0], 20.0)
            .is_none());
        assert!(w
            .raycast([-40.0, 0.0, 10.0], [0.0, 0.0, -1.0], 7.0)
            .is_none());
    }
    #[test]
    fn ray_crosses_grid_cells_to_hit_wall() {
        let w = world(vec![Triangle {
            vertices: [[80.0, -10.0, 0.0], [80.0, 10.0, 0.0], [80.0, 0.0, 10.0]],
            surface: 0,
            model_id: 0,
        }]);
        let h = w
            .raycast([-64.0, 0.0, 2.0], [1.0, 0.0, 0.0], 200.0)
            .unwrap();
        assert!((h.distance - 144.0).abs() < 1e-5);
        assert!(h.normal[0] < -0.99);
    }
    #[test]
    fn active_level_keeps_global_contacts_and_excludes_other_maps() {
        let t = |z| Triangle {
            vertices: [[-10.0, -10.0, z], [10.0, -10.0, z], [0.0, 10.0, z]],
            surface: 0,
            model_id: 0,
        };
        let mut w = world(vec![t(1.0), t(2.0), t(0.0), t(3.0)]);
        w.triangle_levels = vec![1, 2, 0, 250];
        let ray = |w: &CollisionWorld| {
            w.raycast([0.0, 0.0, 10.0], [0.0, 0.0, -1.0], 20.0)
                .unwrap()
                .position[2]
        };
        assert_eq!(ray(&w), 3.0);
        w.set_active_level(Some(1));
        assert_eq!(ray(&w), 1.0);
        w.set_active_level(Some(2));
        assert_eq!(ray(&w), 2.0);
        w.set_active_level(Some(3));
        assert_eq!(ray(&w), 0.0);
        w.set_active_levels(&[1, 2]);
        assert_eq!(ray(&w), 2.0);
    }
    #[test]
    fn compressed_indices_are_byte_offsets_and_bad_pointers_fail() {
        let mut data = vec![0; 256];
        data[32 + 0x32..32 + 0x34].copy_from_slice(&1u16.to_le_bytes());
        data[32 + 0x48..32 + 0x4c].copy_from_slice(&160u32.to_le_bytes());
        data[32 + 0x4c..32 + 0x50].copy_from_slice(&192u32.to_le_bytes());
        data[166..168].copy_from_slice(&128i16.to_le_bytes());
        data[174..176].copy_from_slice(&128i16.to_le_bytes());
        data[194..196].copy_from_slice(&6u16.to_le_bytes());
        data[196..198].copy_from_slice(&12u16.to_le_bytes());
        let m = decode_model(&data, 32).unwrap();
        assert_eq!(
            m.triangles[0].0,
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        );
        data[194..196].copy_from_slice(&7u16.to_le_bytes());
        assert!(decode_model(&data, 32).is_err());
        data[194..196].copy_from_slice(&6u16.to_le_bytes());
        data[32 + 0x48..32 + 0x4c].copy_from_slice(&250u32.to_le_bytes());
        assert!(decode_model(&data, 32).is_err());
    }
    #[test]
    fn batched_queries_match_individual_with_moving_props_and_actors() {
        let mut world = CollisionWorld::from_triangles(Vec::new());
        let shape = test_box_collider([[-1., -1., 0.], [1., 1., 3.]]);
        world.test_add_entity(42, shape.clone(), 1);
        world.test_add_entity(43, test_box_collider([[4., -1., 0.], [5., 1., 2.]]), 2);
        world.awaken(42).unwrap();
        world.move_entity(
            42,
            [
                1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 1., 1.,
            ],
        );
        world.set_ignores_actors(42, true);
        let instances = [shape.at([0., 5., 0.], 0.3)];
        let mut rays = Vec::new();
        for x in -6..=6 {
            for y in -6..=6 {
                rays.push(Ray {
                    origin: [x as f32, y as f32, 1.2],
                    direction: [0.3, 1., 0.],
                    distance: 10.,
                });
                rays.push(Ray {
                    origin: [x as f32, y as f32, 6.],
                    direction: [0., 0., -2.],
                    distance: 10.,
                });
            }
        }
        rays.push(Ray {
            origin: [0.; 3],
            direction: [f32::NAN, 0., 0.],
            distance: 3.,
        });
        rays.push(Ray {
            origin: [0.; 3],
            direction: [1., 0., 0.],
            distance: -1.,
        });
        for level in [None, Some(1), Some(2)] {
            world.set_active_level(level);
            let scene = CollisionScene {
                world: &world,
                instances: &instances,
            };
            let actors = ActorQuery(&scene);
            for query in [&scene as &dyn CollisionQuery, &actors] {
                for (r, h) in rays.iter().zip(query.raycast_batch(&rays)) {
                    let individual = query.raycast(r.origin, r.direction, r.distance);
                    assert_eq!(h.is_some(), individual.is_some());
                    if let (Some(a), Some(b)) = (h, individual) {
                        assert!((a.distance - b.distance).abs() < 0.0001);
                        assert_eq!(a.entity, b.entity);
                    }
                }
            }
        }
    }
    #[test]
    fn exact_scaled_sphere_returns_physical_distance() {
        let s = Sphere {
            center: [0.0; 3],
            radius: 1.0,
            inverse: [[0.5, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            origin: [0.0; 3],
            bounds: [[-2.0, -1.0, -1.0], [2.0, 1.0, 1.0]],
            surface: 1,
            model_id: 8,
            level: 0,
            entity: None,
        };
        let h = ray_sphere([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0], 10.0, &s).unwrap();
        assert_eq!(h.distance, 3.0);
        assert_eq!(h.normal, [-1.0, 0.0, 0.0]);
    }
}
