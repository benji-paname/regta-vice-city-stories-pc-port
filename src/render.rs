use crate::iso::Result;
use crate::leeds::{Instance, Level, Vertex};
use crate::texture::Texture;
use minifb::{Key, Window, WindowOptions};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 540;
pub const CAMERA_NEAR: f32 = 0.5;
pub const CAMERA_FOCAL: f32 = 650.0;
#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self::for_zone(0)
    }
}
impl Camera {
    pub fn walking_spawn(index: usize) -> Self {
        let mut camera = Self::for_zone(index);
        camera.pos = match index {
            1 => [-1000.0, -500.0, 30.0],
            2 => [390.0, 1130.0, -193.0],
            _ => [500.0, -500.0, 30.0],
        };
        camera.pitch = -0.03;
        camera
    }
    pub fn for_zone(index: usize) -> Self {
        match index {
            1 => Self {
                pos: [-1200.0, -900.0, 120.0],
                yaw: 0.25,
                pitch: -0.30,
            },
            2 => Self {
                pos: [390.0, 1130.0, -193.0],
                yaw: 0.0,
                pitch: -0.05,
            },
            _ => Self {
                pos: [500.0, -1000.0, 100.0],
                yaw: -1.1,
                pitch: -0.22,
            },
        }
    }
    pub fn military(walking: bool) -> Self {
        Self {
            pos: [-1460.0, -440.0, if walking { 30.0 } else { 95.0 }],
            yaw: -0.7,
            pitch: if walking { -0.15 } else { -0.3 },
        }
    }
}
pub fn parked_vehicle(index: usize) -> [f32; 3] {
    match index {
        1 => [-1005.0, -495.0, 11.0],
        2 => [385.0, 1125.0, -193.0],
        _ => [504.0, -496.0, 10.5],
    }
}

#[cfg(test)]
fn find_vehicle_exit(
    world: &impl crate::collision::CollisionQuery,
    car: &crate::driving::Car,
    collider: &crate::collision::NativeCollider,
    bounds: [[f32; 3]; 2],
) -> Option<crate::player::Walker> {
    let instances = [collider.at(car.position, car.heading)];
    let scene = crate::collision::CollisionScene {
        world,
        instances: &instances,
    };
    let (s, c) = car.heading.sin_cos();
    for side in [1.0f32, -1.0] {
        let edge = if side > 0.0 {
            bounds[1][0]
        } else {
            -bounds[0][0]
        };
        let distance = edge + 0.45;
        // A clear destination alone is insufficient: do not teleport through a
        // wall between the door and the exit point. The car itself is excluded.
        let blocked = [0.35f32, 0.8, 1.3].iter().any(|z| {
            world
                .raycast(
                    [car.position[0], car.position[1], car.position[2] + z],
                    [c * side, -s * side, 0.0],
                    distance,
                )
                .is_some()
        });
        if blocked {
            continue;
        }
        let camera = Camera {
            pos: [
                car.position[0] + c * side * distance,
                car.position[1] - s * side * distance,
                car.position[2] + 2.2,
            ],
            yaw: car.heading,
            pitch: -0.03,
        };
        if let Some(walker) = crate::player::Walker::from_camera(&scene, &camera) {
            if (walker.feet[2] - car.position[2]).abs() <= 0.65 && walker.has_clear_body(&scene) {
                return Some(walker);
            }
        }
    }
    None
}

fn advance_access(
    access: &mut Option<crate::access::Access>,
    world: &impl crate::collision::CollisionQuery,
    actor: &crate::character::Character,
    car: &crate::driving::Car,
    model: &crate::vehicle::VehicleModel,
    walker: &mut Option<crate::player::Walker>,
    heading: &mut f32,
    in_vehicle: &mut bool,
    chase: &mut VehicleCamera,
    camera: &mut Camera,
    dt: f32,
) {
    let Some(mut action) = access.take() else {
        return;
    };
    match action.tick(world, actor, car, model, walker, heading, dt) {
        crate::access::Outcome::Running => *access = Some(action),
        crate::access::Outcome::Entered => {
            *in_vehicle = true;
            *chase = VehicleCamera::new(car.heading);
            *camera = chase.update(car, world, 0.);
        }
        crate::access::Outcome::Exited => {
            *in_vehicle = false;
            camera.yaw = car.heading - action.side * 0.95;
            camera.pitch = -0.12;
            if let Some(w) = walker {
                camera.pos = w.eye();
            }
        }
        crate::access::Outcome::Cancelled => {}
    }
}

fn step_ambient(
    world: &crate::collision::CollisionWorld,
    paths: &crate::paths::PathNetwork,
    models: &[crate::character::Character],
    player_model: &crate::character::Character,
    vehicle: &crate::vehicle::VehicleModel,
    car: &crate::driving::Car,
    population: &mut crate::population::Population,
    traffic: &mut crate::traffic::Traffic,
    focus: [f32; 3],
    player: Option<[f32; 3]>,
    dt: f32,
) {
    let mut obstacles = population.colliders(models);
    obstacles.push(vehicle.collision.at(car.position, car.heading));
    if let Some(feet) = player {
        obstacles.push(player_model.collision.at(feet, 0.0));
    }
    traffic.step(world, paths, &vehicle.collision, focus, &obstacles, dt);
    let cars: Vec<_> = std::iter::once(*car)
        .chain(traffic.cars.iter().map(|v| v.car))
        .collect();
    population.step(
        world,
        paths,
        &cars,
        &vehicle.collision,
        focus,
        player,
        models.len(),
        dt,
    );
}

struct VehicleCamera {
    yaw: f32,
    distance: f32,
}
impl VehicleCamera {
    fn new(heading: f32) -> Self {
        Self {
            yaw: heading,
            distance: 6.5,
        }
    }
    fn update(
        &mut self,
        car: &crate::driving::Car,
        world: &impl crate::collision::CollisionQuery,
        dt: f32,
    ) -> Camera {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let difference = (car.heading - self.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.yaw += difference * (1.0 - (-7.0 * dt).exp());
        self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let pitch: f32 = -0.25;
        let (s, c) = self.yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let backward = [-s * cp, -c * cp, -sp];
        let target = [car.position[0], car.position[1], car.position[2] + 1.05];
        let wanted = 6.5 + (car.speed.abs() / 58.0).clamp(0.0, 1.0) * 1.2;
        let clearance = camera_clearance(world, target, self.yaw, pitch, wanted, 0.35);
        // Retract immediately for obstacles; extend smoothly once the view clears.
        self.distance =
            (self.distance + (wanted - self.distance) * (1.0 - (-4.0 * dt).exp())).min(clearance);
        Camera {
            pos: std::array::from_fn(|i| target[i] + backward[i] * self.distance),
            yaw: self.yaw,
            pitch,
        }
    }
}

/// Sweep the center, edge midpoints and corners of the near-plane footprint.
/// A center ray alone misses a pillar/overhang touching the edge of the image.
/// The basis follows camera pitch as well as yaw. These discrete probes are a
/// conservative approximation, not a continuous sphere/triangle sweep.
fn camera_clearance(
    world: &impl crate::collision::CollisionQuery,
    target: [f32; 3],
    yaw: f32,
    pitch: f32,
    wanted: f32,
    margin: f32,
) -> f32 {
    let (s, c) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let backward = [-s * cp, -c * cp, -sp];
    let right = [c, -s, 0.0];
    let up = [-sp * s, -sp * c, cp];
    let half_width = CAMERA_NEAR * WIDTH as f32 / (2.0 * CAMERA_FOCAL) + 0.04;
    let half_height = CAMERA_NEAR * HEIGHT as f32 / (2.0 * CAMERA_FOCAL) + 0.04;
    let mut clearance = wanted;
    for x in [-half_width, 0.0, half_width] {
        for y in [-half_height, 0.0, half_height] {
            let origin = std::array::from_fn(|i| target[i] + right[i] * x + up[i] * y);
            if let Some(hit) = world.raycast(origin, backward, clearance + margin) {
                clearance = clearance.min((hit.distance - margin).max(0.0));
            }
        }
    }
    clearance
}

/// Camera follows a feet position, stopping before an original static obstacle.
pub fn follow_camera(
    camera: &Camera,
    feet: [f32; 3],
    collisions: &impl crate::collision::CollisionQuery,
) -> Camera {
    let target = [feet[0], feet[1], feet[2] + 1.1];
    let (s, c) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    let backwards = [-s * cp, -c * cp, -sp];
    // Do not force a minimum distance: that can put the camera through an
    // obstacle closer than the minimum when standing beside a wall.
    let distance = camera_clearance(collisions, target, camera.yaw, camera.pitch, 4.0, 0.25);
    Camera {
        pos: std::array::from_fn(|i| target[i] + backwards[i] * distance),
        ..*camera
    }
}
pub struct Frame {
    pub pixels: Vec<u32>,
    depth: Vec<f32>,
    pub triangles: usize,
    pub textured_triangles: usize,
    scene_instances: [Option<Vec<(usize, usize)>>; 3],
}
impl Frame {
    pub fn new() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT],
            depth: vec![f32::INFINITY; WIDTH * HEIGHT],
            triangles: 0,
            textured_triangles: 0,
            scene_instances: [None, None, None],
        }
    }
    #[allow(dead_code)]
    pub fn draw(&mut self, level: &Level, camera: &Camera) {
        self.draw_mode(level, camera, true);
    }
    #[allow(dead_code)]
    pub fn draw_mode(&mut self, level: &Level, camera: &Camera, textures: bool) {
        self.clear();
        for instance in &level.instances {
            if !crate::scene::is_blended(instance) {
                self.draw_instance(level, instance, camera, textures);
            }
        }
        let mut blended: Vec<_> = level
            .instances
            .iter()
            .filter(|i| crate::scene::is_blended(i))
            .collect();
        blended.sort_by(|a, b| {
            crate::scene::distance_squared(b, camera)
                .total_cmp(&crate::scene::distance_squared(a, camera))
        });
        for instance in blended {
            self.draw_instance(level, instance, camera, textures);
        }
    }
    #[allow(dead_code)]
    pub fn draw_scene(&mut self, levels: &[Level], zone: usize, camera: &Camera, textures: bool) {
        self.draw_scene_character(levels, zone, camera, textures, None);
    }
    pub fn draw_scene_character(
        &mut self,
        levels: &[Level],
        zone: usize,
        camera: &Camera,
        textures: bool,
        character: Option<(
            &crate::gpu::CharacterFrame<'_>,
            &std::collections::HashMap<u16, Texture>,
        )>,
    ) {
        self.draw_scene_all(levels, zone, camera, textures, character, None);
    }

    pub fn draw_scene_all(
        &mut self,
        levels: &[Level],
        zone: usize,
        camera: &Camera,
        textures: bool,
        character: Option<(
            &crate::gpu::CharacterFrame<'_>,
            &std::collections::HashMap<u16, Texture>,
        )>,
        vehicle: Option<(
            &crate::gpu::VehicleFrame<'_>,
            &std::collections::HashMap<u16, Texture>,
        )>,
    ) {
        let actors: Vec<_> = character.into_iter().collect();
        let vehicles: Vec<_> = vehicle.into_iter().collect();
        self.draw_scene_actors(levels, zone, camera, textures, &actors, &vehicles);
    }

    fn draw_scene_actors(
        &mut self,
        levels: &[Level],
        zone: usize,
        camera: &Camera,
        textures: bool,
        characters: &[(
            &crate::gpu::CharacterFrame<'_>,
            &std::collections::HashMap<u16, Texture>,
        )],
        vehicles: &[(
            &crate::gpu::VehicleFrame<'_>,
            &std::collections::HashMap<u16, Texture>,
        )],
    ) {
        self.clear();
        if self.scene_instances[zone].is_none() {
            self.scene_instances[zone] = Some(crate::scene::unique_instances(levels, zone));
        }
        let instances = self.scene_instances[zone].as_ref().unwrap().clone();
        let mut blended = Vec::new();
        for (index, instance_index) in instances {
            let level = &levels[index];
            let instance = &level.instances[instance_index];
            if crate::scene::is_blended(instance) {
                blended.push((level, instance));
            } else {
                self.draw_instance(level, instance, camera, textures);
            }
        }
        for (character, character_textures) in characters {
            let (s, c) = character.heading.sin_cos();
            for triangle in character.triangles {
                let points = triangle.vertices.map(|v| Vertex {
                    p: view(
                        [
                            character.position[0] + c * v.p[0] + s * v.p[1],
                            character.position[1] - s * v.p[0] + c * v.p[1],
                            character.position[2] + v.p[2],
                        ],
                        camera,
                    ),
                    ..v
                });
                let clipped = clip_near(&points, CAMERA_NEAR);
                if clipped.len() < 3 {
                    continue;
                }
                for i in 1..clipped.len() - 1 {
                    self.triangle(
                        [clipped[0], clipped[i], clipped[i + 1]],
                        if textures {
                            character_textures.get(&triangle.texture_id)
                        } else {
                            None
                        },
                        false,
                    );
                }
            }
        }
        for (vehicle, vehicle_textures) in vehicles {
            let (s, c) = vehicle.heading.sin_cos();
            for triangle in vehicle.triangles {
                let points = triangle.vertices.map(|v| Vertex {
                    p: view(
                        [
                            vehicle.position[0] + c * v.p[0] + s * v.p[1],
                            vehicle.position[1] - s * v.p[0] + c * v.p[1],
                            vehicle.position[2] + v.p[2],
                        ],
                        camera,
                    ),
                    ..v
                });
                let clipped = clip_near(&points, CAMERA_NEAR);
                if clipped.len() < 3 {
                    continue;
                }
                for i in 1..clipped.len() - 1 {
                    self.triangle(
                        [clipped[0], clipped[i], clipped[i + 1]],
                        if textures {
                            vehicle_textures.get(&triangle.texture_id)
                        } else {
                            None
                        },
                        false,
                    );
                }
            }
        }
        blended.sort_by(|a, b| {
            crate::scene::distance_squared(b.1, camera)
                .total_cmp(&crate::scene::distance_squared(a.1, camera))
        });
        for (level, instance) in blended {
            self.draw_instance(level, instance, camera, textures);
        }
    }
    fn clear(&mut self) {
        for y in 0..HEIGHT {
            let t = y as f32 / HEIGHT as f32;
            let color = if y < HEIGHT / 2 {
                rgb([0.36 + t * 0.3, 0.60 + t * 0.2, 0.79 + t * 0.2])
            } else {
                0x39687b
            };
            self.pixels[y * WIDTH..(y + 1) * WIDTH].fill(color);
        }
        self.depth.fill(f32::INFINITY);
        self.triangles = 0;
        self.textured_triangles = 0;
    }
    fn draw_instance(
        &mut self,
        level: &Level,
        instance: &Instance,
        camera: &Camera,
        textures: bool,
    ) {
        if !level.visible(instance, 12) || !crate::scene::instance_in_view(instance, camera) {
            return;
        }
        let Some(mesh) = level.meshes.get(&instance.resource) else {
            return;
        };
        let blended = crate::scene::is_blended(instance);
        for triangle in &mesh.triangles {
            let points = triangle.vertices.map(|vertex| Vertex {
                p: view(world(vertex.p, instance), camera),
                color: vertex.color,
                uv: vertex.uv,
            });
            let clipped = clip_near(&points, CAMERA_NEAR);
            if clipped.len() < 3 {
                continue;
            }
            for i in 1..clipped.len() - 1 {
                self.triangle(
                    [clipped[0], clipped[i], clipped[i + 1]],
                    if textures {
                        level.textures.get(&triangle.texture_id)
                    } else {
                        None
                    },
                    blended,
                );
            }
        }
    }
    fn triangle(&mut self, tri: [Vertex; 3], texture: Option<&Texture>, blended: bool) {
        let projected = tri.map(|v| {
            [
                WIDTH as f32 / 2.0 + v.p[0] * CAMERA_FOCAL / v.p[2],
                HEIGHT as f32 / 2.0 - v.p[1] * CAMERA_FOCAL / v.p[2],
            ]
        });
        let area = edge(projected[0], projected[1], projected[2]);
        if !area.is_finite() || area.abs() < 0.05 {
            return;
        }
        let minx = projected
            .iter()
            .map(|p| p[0])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let maxx = projected
            .iter()
            .map(|p| p[0])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(WIDTH as f32 - 1.0)
            .max(0.0) as usize;
        let miny = projected
            .iter()
            .map(|p| p[1])
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize;
        let maxy = projected
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(HEIGHT as f32 - 1.0)
            .max(0.0) as usize;
        if minx > maxx || miny > maxy {
            return;
        }
        self.triangles += 1;
        if texture.is_some() {
            self.textured_triangles += 1;
        }
        let inverse = [1.0 / tri[0].p[2], 1.0 / tri[1].p[2], 1.0 / tri[2].p[2]];
        let lod = texture.map_or(0.0, |texture| {
            let dx1 = projected[1][0] - projected[0][0];
            let dy1 = projected[1][1] - projected[0][1];
            let dx2 = projected[2][0] - projected[0][0];
            let dy2 = projected[2][1] - projected[0][1];
            let det = dx1 * dy2 - dx2 * dy1;
            let du1 = tri[1].uv[0] - tri[0].uv[0];
            let dv1 = tri[1].uv[1] - tri[0].uv[1];
            let du2 = tri[2].uv[0] - tri[0].uv[0];
            let dv2 = tri[2].uv[1] - tri[0].uv[1];
            let dudx = (du1 * dy2 - du2 * dy1) / det;
            let dudy = (du2 * dx1 - du1 * dx2) / det;
            let dvdx = (dv1 * dy2 - dv2 * dy1) / det;
            let dvdy = (dv2 * dx1 - dv1 * dx2) / det;
            let rho_x = ((dudx * texture.width as f32).powi(2)
                + (dvdx * texture.height as f32).powi(2))
            .sqrt();
            let rho_y = ((dudy * texture.width as f32).powi(2)
                + (dvdy * texture.height as f32).powi(2))
            .sqrt();
            rho_x
                .max(rho_y)
                .max(1.0)
                .log2()
                .min(texture.mip_levels() as f32 - 1.0)
        });
        for y in miny..=maxy {
            for x in minx..=maxx {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let weights = [
                    edge(projected[1], projected[2], p) / area,
                    edge(projected[2], projected[0], p) / area,
                    edge(projected[0], projected[1], p) / area,
                ];
                if weights.iter().any(|&w| w < 0.0) {
                    continue;
                }
                let inv =
                    weights[0] * inverse[0] + weights[1] * inverse[1] + weights[2] * inverse[2];
                let depth = 1.0 / inv;
                let index = y * WIDTH + x;
                if depth >= self.depth[index] {
                    continue;
                }
                let fog = ((depth - 650.0) / 3850.0).clamp(0.0, 1.0);
                let sample = if let Some(texture) = texture {
                    let uv = std::array::from_fn(|axis| {
                        (weights[0] * tri[0].uv[axis] * inverse[0]
                            + weights[1] * tri[1].uv[axis] * inverse[1]
                            + weights[2] * tri[2].uv[axis] * inverse[2])
                            / inv
                    });
                    texture.sample_lod(uv, lod)
                } else {
                    [1.0; 4]
                };
                if sample[3] < if blended { 0.01 } else { 0.5 } {
                    continue;
                }
                if !blended {
                    self.depth[index] = depth;
                }
                let mut color = [0.0; 3];
                for axis in 0..3 {
                    color[axis] = (weights[0] * tri[0].color[axis] * inverse[0]
                        + weights[1] * tri[1].color[axis] * inverse[1]
                        + weights[2] * tri[2].color[axis] * inverse[2])
                        / inv;
                    color[axis] =
                        color[axis] * sample[axis] * (1.0 - fog) + [0.58, 0.72, 0.80][axis] * fog;
                }
                self.pixels[index] = if blended {
                    blend_pixels(rgb(color), self.pixels[index], sample[3])
                } else {
                    rgb(color)
                };
            }
        }
    }
    pub fn write_bmp(&self, path: &Path) -> Result<()> {
        let stride = (WIDTH * 3 + 3) & !3;
        let size = 54 + stride * HEIGHT;
        let mut file = std::fs::File::create(path)?;
        let mut header = [0u8; 54];
        header[0..2].copy_from_slice(b"BM");
        header[2..6].copy_from_slice(&(size as u32).to_le_bytes());
        header[10..14].copy_from_slice(&54u32.to_le_bytes());
        header[14..18].copy_from_slice(&40u32.to_le_bytes());
        header[18..22].copy_from_slice(&(WIDTH as i32).to_le_bytes());
        header[22..26].copy_from_slice(&(HEIGHT as i32).to_le_bytes());
        header[26..28].copy_from_slice(&1u16.to_le_bytes());
        header[28..30].copy_from_slice(&24u16.to_le_bytes());
        file.write_all(&header)?;
        for y in (0..HEIGHT).rev() {
            let mut row = vec![0; stride];
            for x in 0..WIDTH {
                let c = self.pixels[y * WIDTH + x];
                row[x * 3] = c as u8;
                row[x * 3 + 1] = (c >> 8) as u8;
                row[x * 3 + 2] = (c >> 16) as u8;
            }
            file.write_all(&row)?;
        }
        Ok(())
    }
    pub fn covered_pixels(&self) -> usize {
        self.depth.iter().filter(|z| z.is_finite()).count()
    }
}
fn rgb(c: [f32; 3]) -> u32 {
    ((c[0].clamp(0.0, 1.0) * 255.0) as u32) << 16
        | ((c[1].clamp(0.0, 1.0) * 255.0) as u32) << 8
        | (c[2].clamp(0.0, 1.0) * 255.0) as u32
}
fn blend_pixels(source: u32, destination: u32, alpha: f32) -> u32 {
    let alpha = alpha.clamp(0.0, 1.0);
    [16, 8, 0]
        .iter()
        .map(|shift| {
            let a = ((source >> shift) & 255) as f32;
            let b = ((destination >> shift) & 255) as f32;
            ((a * alpha + b * (1.0 - alpha)).round() as u32) << shift
        })
        .sum()
}
fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
}
fn world(p: [f32; 3], inst: &Instance) -> [f32; 3] {
    std::array::from_fn(|r| {
        inst.matrix[r] * p[0]
            + inst.matrix[4 + r] * p[1]
            + inst.matrix[8 + r] * p[2]
            + inst.matrix[12 + r]
    })
}
fn view(p: [f32; 3], cam: &Camera) -> [f32; 3] {
    let d = [p[0] - cam.pos[0], p[1] - cam.pos[1], p[2] - cam.pos[2]];
    let (s, c) = cam.yaw.sin_cos();
    let x = c * d[0] - s * d[1];
    let z = s * d[0] + c * d[1];
    let (sp, cp) = cam.pitch.sin_cos();
    [x, cp * d[2] - sp * z, sp * d[2] + cp * z]
}
fn clip_near(tri: &[Vertex; 3], near: f32) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let a = tri[i];
        let b = tri[(i + 1) % 3];
        let inside = a.p[2] >= near;
        let other = b.p[2] >= near;
        if inside {
            out.push(a);
        }
        if inside != other {
            let t = (near - a.p[2]) / (b.p[2] - a.p[2]);
            out.push(Vertex {
                p: std::array::from_fn(|i| a.p[i] + t * (b.p[i] - a.p[i])),
                color: std::array::from_fn(|i| a.color[i] + t * (b.color[i] - a.color[i])),
                uv: std::array::from_fn(|i| a.uv[i] + t * (b.uv[i] - a.uv[i])),
            });
        }
    }
    out
}

pub struct ViewerOptions {
    pub live_report: Option<PathBuf>,
    pub smoke: bool,
    pub initial: usize,
    pub start_walking: bool,
    pub start_driving: bool,
    pub software: bool,
    pub capture: Option<PathBuf>,
    pub headless: bool,
    pub initial_camera: Option<Camera>,
    pub third_person: bool,
    pub animation: Option<String>,
    pub animation_time: Option<f32>,
    pub simulate_seconds: f32,
    pub simulate_jump: bool,
    pub simulate_motion: u8,
    pub simulate_combat: bool,
    pub simulate_vehicle: Option<String>,
    pub simulate_crouch: bool,
    pub simulate_vault: bool,
    pub gameplay: Option<crate::settings::Gameplay>,
    pub show_options: bool,
    pub capture_view: Option<[f32; 2]>,
}
pub fn viewer(
    levels: &mut [Level],
    collisions: &mut crate::collision::CollisionWorld,
    objects: &mut crate::objects::Objects,
    water: &crate::water::WaterGrid,
    hud: &crate::hud::GameHud,
    character: &crate::character::Character,
    vehicle: &crate::vehicle::VehicleModel,
    npc_models: &[crate::character::Character],
    paths: &crate::paths::PathNetwork,
    options: ViewerOptions,
) -> Result<serde_json::Value> {
    let ViewerOptions {
        live_report,
        smoke,
        initial,
        start_walking,
        start_driving,
        software,
        capture,
        headless,
        initial_camera,
        mut third_person,
        animation,
        animation_time,
        simulate_seconds,
        simulate_jump,
        simulate_motion,
        simulate_combat,
        simulate_vehicle,
        simulate_crouch,
        simulate_vault,
        gameplay,
        show_options,
        capture_view,
    } = options;
    let mut window = Window::new(
        "VCS Rust | Chargement du rendu",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(60);
    // Keep the interactive viewer visible even when the launcher hides its console.
    #[cfg(windows)]
    unsafe {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn ShowWindow(window: *mut std::ffi::c_void, command: i32) -> i32;
        }
        ShowWindow(window.get_window_handle(), if headless { 0 } else { 5 });
    }
    let mut gpu = if software {
        None
    } else {
        match crate::gpu::GpuRenderer::new(&window, levels) {
            Ok(renderer) => {
                println!("Graphics driver: {}", renderer.driver);
                Some(renderer)
            }
            Err(error) => {
                eprintln!("GPU renderer unavailable, using native software fallback: {error}");
                None
            }
        }
    };
    let mut frame = Frame::new();
    let mut actor_textures = character.textures.clone();
    for model in npc_models {
        actor_textures.extend(model.textures.clone());
    }
    if let Some(gpu) = &mut gpu {
        gpu.offscreen = headless;
        gpu.set_character_textures(&actor_textures)?;
        gpu.set_vehicle_textures(&vehicle.textures)?;
    }
    crate::scene::select_collisions(collisions, initial);
    let mut camera = initial_camera.unwrap_or_else(|| {
        if start_walking {
            Camera::walking_spawn(initial)
        } else {
            Camera::for_zone(initial)
        }
    });
    let mut walker = if start_walking {
        crate::player::Walker::from_camera_with_water(
            collisions,
            &camera,
            (initial != 2).then_some(water),
        )
    } else {
        None
    };
    if let Some(walker) = &walker {
        camera.pos = walker.eye();
    }
    let mut index = initial;
    let mut car = crate::driving::Car::new(parked_vehicle(index), -1.1);
    car.settle_on_ground(collisions, true, 0.0);
    let mut in_vehicle = start_driving;
    let mut chase_camera = VehicleCamera::new(car.heading);
    let mut population = crate::population::Population::new();
    let mut traffic = crate::traffic::Traffic::new();
    let focus = walker.as_ref().map_or(car.position, |w| w.feet);
    let instances = [vehicle.collision.at(car.position, car.heading)];
    let initial_scene = crate::collision::CollisionScene {
        world: collisions,
        instances: &instances,
    };
    population.refresh(&initial_scene, paths, focus, npc_models.len());
    traffic.refresh(collisions, paths, focus, &instances);
    if in_vehicle {
        walker = None;
    }
    let mut locomotion = crate::locomotion::Locomotion::new(character)?;
    let combat_profile = crate::combat::Profile::load(character)?;
    let mut fighter = crate::combat::Fighter::default();
    let mut player_status = crate::hud::PlayerStatus::default();
    let mut arsenal = crate::weapons::WeaponArsenal::new();
    let mut police = crate::police::WantedSystem::new();
    let mut script = crate::script::MissionScriptSystem::new();
    let mut audio = crate::audio::AudioEngine::new();
    let mut weather = crate::weather::WeatherSystem::new();
    let save_path = std::env::current_exe()?.with_file_name("VCS-Save.json");
    let mut banner_message = String::new();
    let mut banner_timer = 0.0f32;
    let mut clock_accumulator = 0.0f32;
    crate::access::Access::validate(character)?;
    let settings_path = std::env::current_exe()?.with_file_name("VCS-options.json");
    let mut settings = if headless {
        crate::settings::Settings::default()
    } else {
        crate::settings::Settings::load(&settings_path)
    };
    if let Some(mode) = gameplay {
        settings.gameplay = mode;
    }
    let mut options_open = show_options;
    let mut options_message = String::new();
    let mut crouch = crate::modern::Crouch::new(character);
    let mut vault: Option<crate::modern::Vault> = None;
    let mut access: Option<crate::access::Access> = None;
    let mut heading = camera.yaw;
    if let Some(action) = simulate_vehicle.as_deref() {
        let mut solid = traffic.colliders(&vehicle.collision);
        solid.extend(population.colliders(npc_models));
        let scene = crate::collision::CollisionScene {
            world: collisions,
            instances: &solid,
        };
        if action == "enter" {
            let destination = crate::access::Access::begin(&scene, &car, vehicle, None)
                .ok_or("No clear door for entry simulation")?;
            walker = crate::player::Walker::from_camera(
                &scene,
                &Camera {
                    pos: [
                        destination.anchor[0],
                        destination.anchor[1],
                        destination.anchor[2] + 2.,
                    ],
                    yaw: car.heading,
                    pitch: 0.,
                },
            );
            in_vehicle = false;
        } else {
            walker = None;
            in_vehicle = true;
        }
        access = crate::access::Access::begin(&scene, &car, vehicle, walker.as_ref());
        if access.is_none() {
            return Err("No valid vehicle transition".into());
        }
        camera.yaw = car.heading - 0.85;
        camera.pitch = -0.12;
        heading = car.heading;
    }
    if simulate_crouch {
        crouch.wanted = settings.gameplay.modern();
    }
    if simulate_vault && settings.gameplay.modern() {
        let mut solid = traffic.colliders(&vehicle.collision);
        solid.push(vehicle.collision.at(car.position, car.heading));
        let scene = crate::collision::CollisionScene {
            world: collisions,
            instances: &solid,
        };
        vault = walker
            .as_ref()
            .and_then(|w| crate::modern::Vault::begin(&scene, w, heading));
        if vault.is_none() {
            return Err("No traversable low ledge at simulation start".into());
        }
    }
    if simulate_combat {
        let w = walker
            .as_ref()
            .ok_or("Combat simulation requires a grounded player")?;
        let ped = population
            .pedestrians
            .first_mut()
            .ok_or("No pedestrian available for combat simulation")?;
        let (s, c) = camera.yaw.sin_cos();
        let position = [w.feet[0] + s * 1.3, w.feet[1] + c * 1.3, w.feet[2] + 2.];
        ped.walker = crate::player::Walker::from_camera(
            &initial_scene,
            &Camera {
                pos: position,
                yaw: 0.,
                pitch: 0.,
            },
        )
        .ok_or("Invalid combat simulation floor")?;
        if !ped.walker.has_clear_body(&initial_scene) {
            return Err("Obstructed combat simulation location".into());
        }
        ped.heading = camera.yaw + std::f32::consts::PI;
    }
    let simulation_started = Instant::now();
    for simulation_tick in 0..(simulate_seconds * 120.0).ceil() as usize {
        if access.is_some() {
            let mut solid = traffic.colliders(&vehicle.collision);
            solid.extend(population.colliders(npc_models));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &solid,
            };
            advance_access(
                &mut access,
                &scene,
                character,
                &car,
                vehicle,
                &mut walker,
                &mut heading,
                &mut in_vehicle,
                &mut chase_camera,
                &mut camera,
                1. / 120.,
            );
        } else if in_vehicle {
            let mut obstacles = traffic.colliders(&vehicle.collision);
            obstacles.extend(population.colliders(npc_models));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &obstacles,
            };
            car.step(
                &scene,
                if simulate_vehicle.is_some() { 0. } else { 1. },
                0.0,
                0.0,
                1.0 / 120.0,
            );
            objects.impact(collisions, &mut car);
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &obstacles,
            };
            chase_camera.update(&car, &scene, 1.0 / 120.0);
        } else if let Some(walking) = &mut walker {
            let (s, c) = camera.yaw.sin_cos();
            let mut instances = population.colliders(npc_models);
            instances.extend(traffic.colliders(&vehicle.collision));
            instances.push(vehicle.collision.at(car.position, car.heading));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &instances,
            };
            let feet_before = walking.feet;
            crouch.tick(
                character,
                walking,
                &scene,
                settings.gameplay.modern(),
                1. / 120.,
            );
            if let Some(v) = &mut vault {
                if v.tick(&scene, walking, 1. / 120.) {
                    vault = None;
                    locomotion.reset();
                }
            } else {
                locomotion.step(
                    walking,
                    &scene,
                    (index != 2).then_some(water),
                    if simulate_motion == 0
                        || simulate_combat
                        || simulate_vehicle.is_some()
                        || simulate_vault
                    {
                        [0.; 2]
                    } else {
                        [s, c]
                    },
                    if crouch.active() {
                        0.75
                    } else if simulate_motion == 2 || animation.as_deref() == Some("run") {
                        4.5
                    } else {
                        1.8
                    },
                    simulate_jump && simulation_tick == 0 && !crouch.active(),
                    1.0 / 120.0,
                );
            }
            crouch.moved(feet_before, walking.feet);
        }
        let mut solid = traffic.colliders(&vehicle.collision);
        solid.push(vehicle.collision.at(car.position, car.heading));
        let scene = crate::collision::CollisionScene {
            world: collisions,
            instances: &solid,
        };
        population.combat_step(
            &scene,
            &combat_profile,
            walker
                .as_ref()
                .filter(|w| {
                    w.grounded && !locomotion.swimming() && access.is_none() && vault.is_none()
                })
                .map(|w| w.feet),
            camera.yaw,
            &mut fighter,
            simulate_combat && !crouch.active(),
            simulate_combat && !crouch.active(),
            1. / 120.,
        );
        step_ambient(
            collisions,
            paths,
            npc_models,
            character,
            vehicle,
            &car,
            &mut population,
            &mut traffic,
            walker.as_ref().map_or(car.position, |w| w.feet),
            walker.as_ref().map(|w| w.feet),
            1.0 / 120.0,
        );
        for traffic_car in &mut traffic.cars {
            objects.impact(collisions, &mut traffic_car.car);
        }
        let mut blockers = traffic.colliders(&vehicle.collision);
        blockers.extend(population.colliders(npc_models));
        blockers.push(vehicle.collision.at(car.position, car.heading));
        objects.step(levels, collisions, &blockers, 1.0 / 120.0);
        objects.recycle_far(
            levels,
            collisions,
            &blockers,
            walker.as_ref().map_or(car.position, |w| w.feet),
        );
    }
    let simulation_elapsed = simulation_started.elapsed().as_secs_f64();
    if in_vehicle {
        camera = chase_camera.update(&car, collisions, 0.0);
    } else if let Some(walking) = &walker {
        camera.pos = walking.eye();
    }
    if let Some(view) = capture_view {
        camera.yaw = view[0];
        camera.pitch = view[1];
    }
    let mut previous = Instant::now();
    let mut last_live_report = Instant::now();
    let mut ticks = 0;
    let mut textures = true;
    let mut show_help = false;
    let mut physics_time = 0.0;
    let mut pending_jump = false;
    let mut render_seconds = 0.0;
    let mut submitted_triangles = 0;
    let mut animation_clock = 0.0;
    let mut last_clip = "idle";
    let mut destination = initial;
    let mut last_render_camera = camera;
    while window.is_open() {
        let now = Instant::now();
        let was_options = options_open;
        if window.is_key_pressed(Key::F3, minifb::KeyRepeat::No) {
            options_open = !options_open;
            physics_time = 0.;
            pending_jump = false;
        }
        if window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No) {
            if options_open {
                options_open = false;
            } else {
                break;
            }
        }
        if options_open {
            let selected = if window.is_key_pressed(Key::Key1, minifb::KeyRepeat::No) {
                Some(crate::settings::Gameplay::OriginalPs2)
            } else if window.is_key_pressed(Key::Key2, minifb::KeyRepeat::No) {
                Some(crate::settings::Gameplay::Modern)
            } else {
                None
            };
            if let Some(mode) = selected {
                let mut solid = traffic.colliders(&vehicle.collision);
                solid.push(vehicle.collision.at(car.position, car.heading));
                let scene = crate::collision::CollisionScene {
                    world: collisions,
                    instances: &solid,
                };
                if !mode.modern()
                    && (vault.is_some() || walker.as_ref().is_some_and(|w| !w.can_stand(&scene)))
                {
                    options_message = "RELEVEZ-VOUS DANS UN ESPACE DEGAGE".into();
                } else {
                    settings.gameplay = mode;
                    if !mode.modern() {
                        crouch.wanted = false;
                    }
                    options_message = match settings.save(&settings_path) {
                        Ok(()) => "CHOIX MEMORISE".into(),
                        Err(_) => "CHOIX ACTIF - ENREGISTREMENT IMPOSSIBLE".into(),
                    };
                }
            }
        }
        let controls = !options_open && !was_options;
        // Captures preserve the explicitly simulated state, independent of load time.
        let dt = if headless || !controls {
            0.0
        } else {
            now.duration_since(previous).as_secs_f32().min(0.1)
        };
        previous = now;
        if controls && window.is_key_pressed(Key::T, minifb::KeyRepeat::No) {
            textures = !textures;
        }
        if controls && window.is_key_pressed(Key::F1, minifb::KeyRepeat::No) {
            show_help = !show_help;
        }
        if controls && window.is_key_pressed(Key::F2, minifb::KeyRepeat::No) {
            third_person = !third_person;
        }
        for (target, key) in [Key::Key1, Key::Key2, Key::Key3, Key::Key4]
            .iter()
            .enumerate()
        {
            if controls && window.is_key_pressed(*key, minifb::KeyRepeat::No) {
                let walking = walker.is_some() || in_vehicle;
                if in_vehicle {
                    in_vehicle = false;
                }
                destination = target;
                population.reset();
                traffic.reset();
                index = if target == 3 { 1 } else { target };
                car = crate::driving::Car::new(parked_vehicle(index), -1.1);
                crate::scene::select_collisions(collisions, index);
                car.settle_on_ground(collisions, true, 0.0);
                camera = if target == 3 {
                    Camera::military(walking)
                } else if walking {
                    Camera::walking_spawn(index)
                } else {
                    Camera::for_zone(index)
                };
                walker = if walking {
                    crate::player::Walker::from_camera_with_water(
                        collisions,
                        &camera,
                        (index != 2).then_some(water),
                    )
                } else {
                    None
                };
                if let Some(walker) = &walker {
                    camera.pos = walker.eye();
                }
                physics_time = 0.0;
                pending_jump = false;
                heading = camera.yaw;
                animation_clock = 0.0;
                locomotion.reset();
                access = None;
                vault = None;
                crouch.reset();
                fighter = crate::combat::Fighter::default();
                arsenal = crate::weapons::WeaponArsenal::new();
                police.clear();
                script = crate::script::MissionScriptSystem::new();
                weather = crate::weather::WeatherSystem::new();
            }
        }
        if controls
            && access.is_none()
            && vault.is_none()
            && window.is_key_pressed(Key::G, minifb::KeyRepeat::No)
        {
            locomotion.reset();
            access = None;
            vault = None;
            crouch.reset();
            if !in_vehicle && walker.is_some() {
                walker = None;
            } else if !in_vehicle {
                // The original mainland has a map-wide flight ceiling at z107.5.
                // Start the ground query below it, preserving the native collider.
                if index != 2 {
                    camera.pos[2] = camera.pos[2].min(90.0);
                }
                walker = crate::player::Walker::from_camera_with_water(
                    collisions,
                    &camera,
                    (index != 2).then_some(water),
                );
                if let Some(walker) = &walker {
                    camera.pos = walker.eye();
                }
            }
            physics_time = 0.0;
            pending_jump = false;
        }
        if controls && window.is_key_pressed(Key::R, minifb::KeyRepeat::No) {
            locomotion.reset();
            access = None;
            vault = None;
            crouch.reset();
            fighter = crate::combat::Fighter::default();
            arsenal = crate::weapons::WeaponArsenal::new();
            police.clear();
            script = crate::script::MissionScriptSystem::new();
            weather = crate::weather::WeatherSystem::new();
            let walking = walker.is_some() || in_vehicle;
            population.reset();
            traffic.reset();
            in_vehicle = false;
            car = crate::driving::Car::new(parked_vehicle(index), -1.1);
            car.settle_on_ground(collisions, true, 0.0);
            camera = if destination == 3 {
                Camera::military(walking)
            } else if walking {
                Camera::walking_spawn(index)
            } else {
                Camera::for_zone(index)
            };
            walker = if walking {
                crate::player::Walker::from_camera_with_water(
                    collisions,
                    &camera,
                    (index != 2).then_some(water),
                )
            } else {
                None
            };
            if let Some(walker) = &walker {
                camera.pos = walker.eye();
            }
            physics_time = 0.0;
            pending_jump = false;
            heading = camera.yaw;
            animation_clock = 0.0;
        }
        let forward = if controls {
            (window.is_key_down(Key::W) || window.is_key_down(Key::Z)) as i32
                - window.is_key_down(Key::S) as i32
        } else {
            0
        };
        let side = if controls {
            window.is_key_down(Key::D) as i32
                - (window.is_key_down(Key::A) || window.is_key_down(Key::Q)) as i32
        } else {
            0
        };
        let (s, c) = camera.yaw.sin_cos();
        let axis = [
            forward as f32 * s + side as f32 * c,
            forward as f32 * c - side as f32 * s,
        ];
        if controls && access.is_none() && vault.is_none() && !fighter.busy() && fighter.alive() {
            if settings.gameplay.modern()
                && window.is_key_pressed(Key::C, minifb::KeyRepeat::No)
                && walker.as_ref().is_some_and(|w| w.grounded)
                && !locomotion.swimming()
            {
                crouch.wanted = !crouch.wanted;
            }
            if window.is_key_pressed(Key::E, minifb::KeyRepeat::No) && !crouch.active() {
                if in_vehicle
                    || walker
                        .as_ref()
                        .is_some_and(|w| w.grounded && !locomotion.swimming())
                        && traffic.enter_nearest(
                            collisions,
                            &vehicle.collision,
                            &population.colliders(npc_models),
                            walker.as_ref().unwrap().feet,
                            &mut car,
                        )
                {
                    let mut solid = traffic.colliders(&vehicle.collision);
                    solid.extend(population.colliders(npc_models));
                    let scene = crate::collision::CollisionScene {
                        world: collisions,
                        instances: &solid,
                    };
                    access = crate::access::Access::begin(
                        &scene,
                        &car,
                        vehicle,
                        if in_vehicle { None } else { walker.as_ref() },
                    );
                    if access.is_some() {
                        car.speed = 0.;
                        fighter.cancel();
                        physics_time = 0.;
                        pending_jump = false;
                    }
                }
            }
        }
        if access.is_some() {
            let mut solid = traffic.colliders(&vehicle.collision);
            solid.extend(population.colliders(npc_models));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &solid,
            };
            advance_access(
                &mut access,
                &scene,
                character,
                &car,
                vehicle,
                &mut walker,
                &mut heading,
                &mut in_vehicle,
                &mut chase_camera,
                &mut camera,
                dt,
            );
            locomotion.reset();
        } else if in_vehicle {
            let accelerate = window.is_key_down(Key::W)
                || window.is_key_down(Key::Z)
                || window.is_key_down(Key::Up);
            let reverse = window.is_key_down(Key::S) || window.is_key_down(Key::Down);
            let throttle = accelerate as i32 as f32 - reverse as i32 as f32;
            let brake = (window.is_key_down(Key::Space) || (accelerate && reverse)) as i32 as f32;
            let steer = (window.is_key_down(Key::Right) || window.is_key_down(Key::D)) as i32
                as f32
                - (window.is_key_down(Key::Left)
                    || window.is_key_down(Key::A)
                    || window.is_key_down(Key::Q)) as i32 as f32;
            physics_time += dt;
            let mut obstacles = traffic.colliders(&vehicle.collision);
            obstacles.extend(population.colliders(npc_models));
            while physics_time >= 1.0 / 120.0 {
                let scene = crate::collision::CollisionScene {
                    world: collisions,
                    instances: &obstacles,
                };
                car.step(&scene, throttle, brake, steer, 1.0 / 120.0);
                objects.impact(collisions, &mut car);
                let mut blockers = obstacles.clone();
                blockers.push(vehicle.collision.at(car.position, car.heading));
                objects.step(levels, collisions, &blockers, 1.0 / 120.0);
                physics_time -= 1.0 / 120.0;
            }
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &obstacles,
            };
            camera = chase_camera.update(&car, &scene, dt);
        } else if let Some(walker) = &mut walker {
            pending_jump |= controls
                && !fighter.busy()
                && !crouch.active()
                && window.is_key_pressed(Key::Space, minifb::KeyRepeat::No);
            let speed = if crouch.active() {
                0.75
            } else if window.is_key_down(Key::LeftShift) {
                4.5
            } else {
                1.8
            };
            physics_time += dt;
            let mut instances = population.colliders(npc_models);
            instances.extend(traffic.colliders(&vehicle.collision));
            instances.push(vehicle.collision.at(car.position, car.heading));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &instances,
            };
            while physics_time >= 1.0 / 120.0 {
                let feet_before = walker.feet;
                crouch.tick(
                    character,
                    walker,
                    &scene,
                    settings.gameplay.modern()
                        && walker.grounded
                        && !locomotion.swimming()
                        && !fighter.busy(),
                    1. / 120.,
                );
                if pending_jump && settings.gameplay.modern() && vault.is_none() {
                    vault = crate::modern::Vault::begin(
                        &scene,
                        walker,
                        if forward != 0 || side != 0 {
                            axis[0].atan2(axis[1])
                        } else {
                            heading
                        },
                    );
                    if vault.is_some() {
                        heading = if forward != 0 || side != 0 {
                            axis[0].atan2(axis[1])
                        } else {
                            heading
                        };
                        pending_jump = false;
                    }
                }
                if let Some(v) = &mut vault {
                    if v.tick(&scene, walker, 1. / 120.) {
                        vault = None;
                        locomotion.reset();
                    }
                } else {
                    locomotion.step(
                        walker,
                        &scene,
                        (index != 2).then_some(water),
                        if fighter.busy() { [0.; 2] } else { axis },
                        speed,
                        pending_jump && !fighter.busy(),
                        1.0 / 120.0,
                    );
                }
                crouch.moved(feet_before, walker.feet);
                pending_jump = false;
                physics_time -= 1.0 / 120.0;
            }
            camera.pos = walker.eye();
        } else {
            let speed = if window.is_key_down(Key::LeftShift) {
                220.0
            } else {
                65.0
            };
            camera.pos[0] += axis[0] * dt * speed;
            camera.pos[1] += axis[1] * dt * speed;
            camera.pos[2] += (window.is_key_down(Key::Space) as i32
                - window.is_key_down(Key::LeftCtrl) as i32) as f32
                * dt
                * speed;
        }
        if !in_vehicle && access.is_none() {
            car.step(collisions, 0.0, 0.0, 0.0, dt);
            camera.yaw += (window.is_key_down(Key::Right) as i32
                - window.is_key_down(Key::Left) as i32) as f32
                * dt
                * 1.4;
            camera.pitch = (camera.pitch
                + (window.is_key_down(Key::Up) as i32 - window.is_key_down(Key::Down) as i32)
                    as f32
                    * dt)
                .clamp(-1.4, 1.4);
        }
        let moving =
            !in_vehicle && access.is_none() && vault.is_none() && (forward != 0 || side != 0);
        let focus = walker.as_ref().map_or(car.position, |w| w.feet);
        step_ambient(
            collisions,
            paths,
            npc_models,
            character,
            vehicle,
            &car,
            &mut population,
            &mut traffic,
            focus,
            walker.as_ref().map(|w| w.feet),
            dt,
        );
        arsenal.update(collisions, dt);
        for exp in &arsenal.explosions {
            traffic.apply_explosion_damage(exp.position, exp.radius, exp.damage);
            let dx = car.position[0] - exp.position[0];
            let dy = car.position[1] - exp.position[1];
            let dist = dx.hypot(dy);
            if dist < exp.radius {
                car.apply_damage(exp.damage * (1.0 - dist / exp.radius));
            }
            population.trigger_panic(exp.position, 60.0);
            police.report_crime(crate::police::CrimeType::ExplosionDetonated, exp.position);
        }
        for hit in &arsenal.bullet_hits {
            traffic.apply_bullet_damage(hit.position, hit.damage);
            let dx = car.position[0] - hit.position[0];
            let dy = car.position[1] - hit.position[1];
            if dx.hypot(dy) < 2.5 {
                car.apply_damage(hit.damage);
            }
        }
        for traffic_car in &mut traffic.cars {
            objects.impact(collisions, &mut traffic_car.car);
        }
        if !in_vehicle {
            let mut blockers = traffic.colliders(&vehicle.collision);
            blockers.extend(population.colliders(npc_models));
            blockers.push(vehicle.collision.at(car.position, car.heading));
            if let Some(w) = &walker {
                blockers.push(character.collision.at(w.feet, heading));
            }
            objects.step(levels, collisions, &blockers, dt);
        }
        {
            let mut blockers = traffic.colliders(&vehicle.collision);
            blockers.extend(population.colliders(npc_models));
            blockers.push(vehicle.collision.at(car.position, car.heading));
            if let Some(w) = &walker {
                blockers.push(character.collision.at(w.feet, heading));
            }
            objects.recycle_far(levels, collisions, &blockers, focus);
        }
        if moving && walker.is_some() && !fighter.busy() {
            heading = axis[0].atan2(axis[1]);
        }
        if controls && window.is_key_pressed(Key::Tab, minifb::KeyRepeat::No) {
            arsenal.cycle_next();
        }
        arsenal.is_aiming = controls
            && (window.is_key_down(Key::V) || window.get_mouse_down(minifb::MouseButton::Right));

        if controls && window.is_key_pressed(Key::H, minifb::KeyRepeat::No) {
            traffic.siren_active = !traffic.siren_active;
        }
        if controls && in_vehicle && window.is_key_pressed(Key::B, minifb::KeyRepeat::No) {
            audio.cycle_radio_next();
        }
        if controls && window.is_key_pressed(Key::K, minifb::KeyRepeat::No) {
            weather.cycle_next();
            banner_message = format!("METEO : {}", weather.current.name());
            banner_timer = 2.5;
        }
        if traffic.siren_active {
            traffic.siren_pos = if in_vehicle { car.position } else { camera.pos };
        }

        let shoot = controls
            && ((arsenal.current != crate::weapons::WeaponType::Fists
                && window.get_mouse_down(minifb::MouseButton::Left))
                || window.is_key_pressed(Key::F, minifb::KeyRepeat::No));
        if shoot {
            let (s, c) = camera.yaw.sin_cos();
            let (sp, cp) = camera.pitch.sin_cos();
            let dir = [s * cp, c * cp, -sp];
            let origin = walker.as_ref().map_or(
                [car.position[0], car.position[1], car.position[2] + 0.8],
                |w| [w.feet[0], w.feet[1], w.feet[2] + 1.2],
            );
            if arsenal.fire(origin, dir, collisions) {
                population.trigger_panic(origin, 45.0);
                police.report_crime(crate::police::CrimeType::FirearmDischarge, origin);
            }
        }

        if controls && window.is_key_pressed(Key::F5, minifb::KeyRepeat::No) {
            let state = crate::save::GameSaveState {
                player_pos: walker.as_ref().map_or(car.position, |w| w.feet),
                player_yaw: camera.yaw,
                player_pitch: camera.pitch,
                health: player_status.health,
                armor: player_status.armor,
                money: player_status.money,
                wanted_level: police.wanted_level,
                current_weapon: arsenal.current.to_index(),
                ammo: arsenal.ammo,
                hour: player_status.hour,
                minute: player_status.minute,
                car_pos: car.position,
                car_heading: car.heading,
                car_health: car.health,
            };
            if state.save_to_file(&save_path).is_ok() {
                banner_message = "PARTIE SAUVEGARDEE [F5]".into();
                banner_timer = 2.5;
            }
        }
        if controls && window.is_key_pressed(Key::F9, minifb::KeyRepeat::No) {
            if let Ok(state) = crate::save::GameSaveState::load_from_file(&save_path) {
                player_status.health = state.health;
                player_status.armor = state.armor;
                player_status.money = state.money;
                player_status.hour = state.hour;
                player_status.minute = state.minute;
                police.set_wanted_level(state.wanted_level);
                arsenal.select(crate::weapons::WeaponType::from_index(state.current_weapon));
                arsenal.ammo = state.ammo;
                car.position = state.car_pos;
                car.heading = state.car_heading;
                car.health = state.car_health;
                car.burning = state.car_health < 250.0 && state.car_health > 0.0;
                car.exploded = state.car_health <= 0.0;
                if let Some(w) = &mut walker {
                    w.feet = state.player_pos;
                }
                camera.yaw = state.player_yaw;
                camera.pitch = state.player_pitch;
                banner_message = "PARTIE RESTAUREE [F9]".into();
                banner_timer = 2.5;
            }
        }

        clock_accumulator += dt * 6.0;
        if clock_accumulator >= 1.0 {
            let added_minutes = clock_accumulator.floor() as u32;
            clock_accumulator -= added_minutes as f32;
            player_status.minute += added_minutes;
            player_status.hour = (player_status.hour + player_status.minute / 60) % 24;
            player_status.minute %= 60;
        }
        audio.update(car.speed, traffic.siren_active, dt);
        let thunder = weather.update(walker.as_ref().map_or(car.position, |w| w.feet), dt);
        if thunder {
            audio.synthesize_thunder(2048);
        }
        if let Some(reward) = script.update(walker.as_ref().map_or(car.position, |w| w.feet), dt) {
            player_status.money += reward;
        }
        player_status.mission_blip = script.marker_position();
        if let Some(radio_text) = audio.radio_banner() {
            banner_message = radio_text;
            banner_timer = 2.0;
        }
        police.update(dt);
        player_status.wanted_level = police.stars_active();
        player_status.wanted_flashing = police.is_flashing();
        if banner_timer > 0.0 {
            banner_timer = (banner_timer - dt).max(0.0);
        }
        if police.wanted_level >= 1 {
            traffic.siren_active = true;
            traffic.siren_pos = walker.as_ref().map_or(car.position, |w| w.feet);
            if let Some(first) = traffic.cars.first_mut() {
                first.car.is_police = true;
            }
        }
        let attack = controls
            && (arsenal.current == crate::weapons::WeaponType::Fists)
            && (window.get_mouse_down(minifb::MouseButton::Left)
                || window.is_key_down(Key::LeftCtrl));
        let guard = controls && window.get_mouse_down(minifb::MouseButton::Right);
        let mut solid = traffic.colliders(&vehicle.collision);
        solid.push(vehicle.collision.at(car.position, car.heading));
        let scene = crate::collision::CollisionScene {
            world: collisions,
            instances: &solid,
        };
        let combat_position = walker
            .as_ref()
            .filter(|w| w.grounded && !locomotion.swimming() && access.is_none() && vault.is_none())
            .map(|w| w.feet);
        if guard && !fighter.busy() {
            if let Some(p) = combat_position {
                let candidates: Vec<_> = population
                    .pedestrians
                    .iter()
                    .filter(|p| p.fighter.alive())
                    .map(|p| p.walker.feet)
                    .collect();
                if let Some(i) = crate::combat::target(&scene, p, camera.yaw, &candidates, 4.) {
                    heading = (candidates[i][0] - p[0]).atan2(candidates[i][1] - p[1]);
                }
            }
        }
        population.combat_step(
            &scene,
            &combat_profile,
            combat_position,
            heading,
            &mut fighter,
            attack && !crouch.active(),
            guard && !moving && !crouch.active(),
            dt,
        );
        player_status.health = fighter.health;
        player_status.armor = fighter.armor;
        if walker.is_some() {
            let pose = locomotion.pose();
            animation_clock = pose.time;
            last_clip = pose.clip;
        } else {
            locomotion.reset();
            animation_clock += dt;
            last_clip = "idle";
        }
        if in_vehicle && access.is_none() {
            last_clip = "CAR_Lsit";
            animation_clock = 0.;
        }
        let show_character = (third_person && walker.is_some()) || access.is_some() || in_vehicle;
        let posed = if show_character {
            if let Some(a) = &access {
                let mut mesh = character.triangles_blended(a.pose(), None);
                if !a.approaching() && a.side < 0. {
                    for t in &mut mesh {
                        for v in &mut t.vertices {
                            v.p[0] = -v.p[0];
                        }
                        t.vertices.swap(1, 2);
                    }
                }
                mesh
            } else if in_vehicle {
                character.triangles_blended(
                    crate::character::Pose {
                        clip: "CAR_Lsit",
                        time: 0.,
                        looping: false,
                        root_height: true,
                    },
                    None,
                )
            } else if let Some(v) = &vault {
                character.triangles_blended(v.pose(character), None)
            } else if fighter.busy() {
                character.triangles_blended(fighter.pose().unwrap(), None)
            } else if crouch.active() {
                character.crouch_triangles(crouch.pose(), crouch.gait, crouch.walking)
            } else if let Some(pose) = fighter.pose() {
                character.triangles_blended(pose, None)
            } else if let Some(clip) = animation.as_deref() {
                character.triangles(clip, animation_time.unwrap_or(animation_clock))
            } else {
                character.triangles_blended(locomotion.pose(), locomotion.blend())
            }
        } else {
            Vec::new()
        };
        let actor_position = access
            .as_ref()
            .map(|a| a.position(character, &car, walker.as_ref()))
            .or_else(|| {
                if in_vehicle {
                    Some(crate::access::Access::seated_position(
                        character, vehicle, &car,
                    ))
                } else {
                    walker.as_ref().map(|w| w.feet)
                }
            });
        let character_frame =
            actor_position
                .filter(|_| show_character)
                .map(|position| crate::gpu::CharacterFrame {
                    triangles: &posed,
                    position,
                    heading: if in_vehicle { car.heading } else { heading },
                });
        let render_camera = if let Some(a) = &access {
            // Frame the whole car and actor while crossing the door; keep the
            // camera outside the body without switching to the driving view mid-clip.
            let yaw = car.heading - a.side * 0.95;
            let pitch = -0.12f32;
            let target = [car.position[0], car.position[1], car.position[2] + 0.8];
            let distance = camera_clearance(collisions, target, yaw, pitch, 6.5, 0.25);
            let (s, c) = yaw.sin_cos();
            let (sp, cp) = pitch.sin_cos();
            Camera {
                pos: [
                    target[0] - s * cp * distance,
                    target[1] - c * cp * distance,
                    target[2] - sp * distance,
                ],
                yaw,
                pitch,
            }
        } else if in_vehicle {
            camera
        } else if show_character {
            let mut instances = traffic.colliders(&vehicle.collision);
            instances.push(vehicle.collision.at(car.position, car.heading));
            let scene = crate::collision::CollisionScene {
                world: collisions,
                instances: &instances,
            };
            let mut focus = actor_position.unwrap();
            if locomotion.swimming() {
                focus[2] += 0.5;
            }
            if let Some(w) = &walker {
                focus[2] -= 0.62 * w.crouch_amount();
            }
            follow_camera(&camera, focus, &scene)
        } else {
            camera
        };
        if let Some(pose) = access
            .as_ref()
            .map(|a| a.pose())
            .or_else(|| vault.as_ref().map(|v| v.pose(character)))
            .or_else(|| if fighter.busy() { fighter.pose() } else { None })
            .or_else(|| {
                if crouch.active() {
                    Some(crouch.pose())
                } else {
                    fighter.pose()
                }
            })
        {
            last_clip = pose.clip;
            animation_clock = pose.time;
        }
        let npc_triangles: Vec<_> = population
            .pedestrians
            .iter()
            .map(|p| {
                if let Some(pose) = p.fighter.pose() {
                    npc_models[p.model].triangles_blended(pose, None)
                } else {
                    npc_models[p.model].triangles(p.clip, p.animation_time)
                }
            })
            .collect();
        let mut actors: Vec<_> = character_frame.into_iter().collect();
        for (ped, mesh) in population.pedestrians.iter().zip(&npc_triangles) {
            actors.push(crate::gpu::CharacterFrame {
                triangles: mesh,
                position: ped.walker.feet,
                heading: ped.heading,
            });
        }
        last_render_camera = render_camera;
        let cars: Vec<_> = std::iter::once(&car)
            .chain(traffic.cars.iter().map(|v| &v.car))
            .collect();
        let mut vehicle_triangles: Vec<_> = cars
            .iter()
            .map(|v| vehicle.animated_triangles_with_suspension(v.wheel_angle, v.steering_angle, v.suspension_displacements()))
            .collect();
        if let Some(a) = &access {
            vehicle.animate_door(&mut vehicle_triangles[0], a.side, a.opening(character));
        }
        let vehicle_frames: Vec<_> = cars
            .iter()
            .zip(&vehicle_triangles)
            .map(|(v, triangles)| crate::gpu::VehicleFrame {
                triangles,
                position: v.position,
                heading: v.heading,
                health: v.health,
                burning: v.burning,
                exploded: v.exploded,
                is_police: v.is_police,
                strobe_timer: v.strobe_timer,
                headlights: v.headlights,
            })
            .collect();
        let draw_started = Instant::now();
        let hud_position = walker
            .as_ref()
            .map_or(if in_vehicle { car.position } else { camera.pos }, |w| {
                w.feet
            });
        let hud_heading = if in_vehicle {
            car.heading
        } else if walker.is_some() {
            heading
        } else {
            camera.yaw
        };
        let mut hud_layers = hud.layers(
            &player_status,
            hud_position,
            render_camera.yaw,
            hud_heading,
            index != 2,
        );
        if options_open {
            hud_layers[2] = crate::hud::options(settings.gameplay, &options_message);
        }
        if let Some(gpu) = &mut gpu {
            gpu.draw(
                levels,
                index,
                &render_camera,
                textures,
                walker.is_some(),
                in_vehicle,
                show_help,
                &actors,
                &vehicle_frames,
                &hud_layers,
                &traffic.lights,
                Some(&arsenal),
                Some(&script),
                Some(&weather),
                traffic.global_time,
            )?;
            submitted_triangles = gpu.triangles;
            render_seconds += draw_started.elapsed().as_secs_f64();
            window.update();
        } else {
            let actor_refs: Vec<_> = actors
                .iter()
                .map(|actor| (actor, &actor_textures))
                .collect();
            frame.draw_scene_actors(
                levels,
                index,
                &render_camera,
                textures,
                &actor_refs,
                &vehicle_frames
                    .iter()
                    .map(|v| (v, &vehicle.textures))
                    .collect::<Vec<_>>(),
            );
            for layer in &hud_layers {
                layer.composite(&mut frame.pixels);
            }
            crate::hud::guide(
                &mut frame.pixels,
                crate::scene::label(index),
                walker.is_some(),
                in_vehicle,
                show_help,
            );
            submitted_triangles = frame.triangles;
            render_seconds += draw_started.elapsed().as_secs_f64();
            window.update_with_buffer(&frame.pixels, WIDTH, HEIGHT)?;
        }
        let title_banner = if banner_timer > 0.0 {
            format!(" | [{banner_message}]")
        } else {
            String::new()
        };
        let wanted_stars = if police.wanted_level > 0 {
            format!(" | Police: {}*", police.wanted_level)
        } else {
            String::new()
        };
        window.set_title(&format!(
            "VCS Rust | {} | {} | {:02}:{:02} | {} | Arme: {}{}{}",
            settings.gameplay.label(),
            crate::scene::label(index),
            player_status.hour,
            player_status.minute,
            if in_vehicle {
                &format!(
                    "Banshee | {:.0} km/h (Santé {:.0}%) | {} | B: radio{}{}",
                    car.speed.abs() * 3.6,
                    (car.health / 10.0).max(0.0),
                    audio.current_station.name(),
                    if car.speed < -0.1 { " [Recul]" } else { "" },
                    if car.exploded { " [DETRUITE]" } else if car.burning { " [EN FEU]" } else { "" },
                )
            } else if walker.is_some() {
                if locomotion.swimming() {
                    "Nage | MAJ: nager plus vite"
                } else {
                    "A pied | ESPACE: sauter"
                }
            } else {
                "Camera libre"
            },
            arsenal.current.name(),
            wanted_stars,
            &format!("{} | {}", weather.current.name(), title_banner),
        ));
        ticks += 1;
        if let Some(path) = live_report
            .as_ref()
            .filter(|_| last_live_report.elapsed().as_secs_f32() >= 1.0)
        {
            let state = serde_json::json!({
                "frame":ticks,"zone":levels[index].stats.name,
                "camera":{"position":render_camera.pos,"yaw":render_camera.yaw,"pitch":render_camera.pitch},
                "walking":walker.is_some(),"swimming":locomotion.swimming(),"hud":player_status,"combat":fighter.snapshot(),"gameplay":settings.gameplay,"crouch":walker.as_ref().map_or(0.,|w|w.crouch_amount()),"vault":vault.as_ref().map(|v|v.snapshot()),"vehicle_access":access.as_ref().map(|a|a.snapshot()),"driving":in_vehicle,
                "feet":walker.as_ref().map(|w|w.feet),"grounded":walker.as_ref().map(|w|w.grounded),
                "vehicle_position":car.position,"vehicle_heading":car.heading,"vehicle_speed":car.speed,
                "vehicle_grounded":car.grounded,"traffic":traffic.snapshot(),"pedestrians":population.snapshot(npc_models),
                "objects":objects.snapshot(),
                "draw_ms":draw_started.elapsed().as_secs_f64()*1000.0,
            });
            if let Ok(data) = serde_json::to_vec_pretty(&state) {
                let _ = std::fs::write(path, data);
            }
            last_live_report = Instant::now();
        }
        if smoke && ticks >= 8 {
            if let Some(path) = capture.as_deref() {
                if let Some(gpu) = &gpu {
                    gpu.write_bmp(path)?;
                } else {
                    frame.write_bmp(path)?;
                }
            }
            println!("Native window smoke test: {ticks} frames, {submitted_triangles} triangles submitted, {:.2} ms average draw",render_seconds*1000.0/ticks as f64);
            break;
        }
    }
    Ok(
        serde_json::json!({"objects":objects.snapshot(),"zone":levels[index].stats.name,"scene":crate::scene::label(index),"scene_levels":crate::scene::level_indices(index),"draw_distance":crate::scene::DRAW_DISTANCE,"renderer":if gpu.is_some(){"OpenGL"}else{"software"},"driver":gpu.as_ref().map(|g|&g.driver),"frames":ticks,"submitted_triangles":submitted_triangles,"pedestrians":population.snapshot(npc_models),"traffic":traffic.snapshot(),"walking":walker.is_some(),"swimming":locomotion.swimming(),"hud":player_status,"combat":fighter.snapshot(),"gameplay":settings.gameplay,"crouch":walker.as_ref().map_or(0.,|w|w.crouch_amount()),"vault":vault.as_ref().map(|v|v.snapshot()),"vehicle_access":access.as_ref().map(|a|a.snapshot()),"driving":in_vehicle,"vehicle_speed":car.speed,"vehicle_position":car.position,"vehicle_grounded":car.grounded,"vehicle_heading":car.heading,"feet":walker.as_ref().map(|w|w.feet),"grounded":walker.as_ref().map(|w|w.grounded),"third_person":third_person,"animation":animation.as_deref().unwrap_or(last_clip),"animation_time":animation_time.unwrap_or(animation_clock),"simulation_seconds":simulate_seconds,"simulation_wall_ms":simulation_elapsed*1000.,"camera":last_render_camera.pos,"average_draw_ms":render_seconds*1000.0/ticks.max(1) as f64}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires the user's local VCS PS2 PAL ISO"]
    fn native_camera_edges_cover_original_map_obstacles() {
        let mut disc = crate::iso::Disc::open(Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        crate::scene::select_collisions(&mut world, 0);
        let paths = crate::paths::PathNetwork::load(&game).unwrap();
        let mut samples = 0;
        let mut improvements = [0usize; 2];
        for node in paths.nodes.iter().skip(paths.car_nodes).step_by(7) {
            let p = node.position;
            let mut camera = Camera {
                pos: [p[0], p[1], p[2] + 3.0],
                yaw: 0.0,
                pitch: -0.15,
            };
            let Some(walker) = crate::player::Walker::from_camera(&world, &camera) else {
                continue;
            };
            if walker.feet[2] < 7.0 || !walker.has_clear_body(&world) {
                continue;
            }
            let target = [walker.feet[0], walker.feet[1], walker.feet[2] + 1.1];
            for i in 0..8 {
                camera.yaw = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, c) = camera.yaw.sin_cos();
                let (sp, cp) = camera.pitch.sin_cos();
                let old = world
                    .raycast(target, [-s * cp, -c * cp, -sp], 4.25)
                    .map_or(4.0, |h| (h.distance - 0.25).clamp(0.2, 4.0));
                let new = camera_clearance(&world, target, camera.yaw, camera.pitch, 4.0, 0.25);
                assert!(new.is_finite() && new >= 0.0 && new <= old + 0.001);
                samples += 1;
                if old - new > 0.5 && new > 0.8 {
                    let island = usize::from(p[0] < -500.0);
                    if improvements[island] < 3 {
                        println!(
                            "Native camera edge: input={},{},{},{},{} old={old} new={new}",
                            camera.pos[0], camera.pos[1], camera.pos[2], camera.yaw, camera.pitch
                        );
                    }
                    improvements[island] += 1;
                }
            }
        }
        println!("Native camera samples={samples}, edge contacts by island={improvements:?}");
        assert!(samples > 1000);
        assert!(improvements.iter().all(|&n| n > 0));
    }

    #[test]
    #[ignore = "Requires the user's local VCS PS2 PAL ISO"]
    fn native_traffic_entry_and_exit_match_original_car_colliders() {
        let mut disc = crate::iso::Disc::open(Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        crate::scene::select_collisions(&mut world, 0);
        let model = crate::vehicle::VehicleModel::load_banshee(&mut disc, &game).unwrap();
        let paths = crate::paths::PathNetwork::load(&game).unwrap();
        let mut player = crate::driving::Car::new(parked_vehicle(0), -1.1);
        player.settle_on_ground(&world, true, 0.0);
        let old_position = player.position;
        let mut traffic = crate::traffic::Traffic::new();
        traffic.refresh(
            &world,
            &paths,
            player.position,
            &[model.collision.at(player.position, player.heading)],
        );
        let candidates: Vec<_> = traffic.cars.iter().map(|v| v.car).collect();
        assert!(
            !candidates.is_empty(),
            "native road graph must produce traffic"
        );
        let mut verified = false;
        for candidate in candidates {
            let mut obstacles: Vec<_> = traffic
                .cars
                .iter()
                .filter(|v| v.car.position != candidate.position)
                .map(|v| model.collision.at(v.car.position, v.car.heading))
                .collect();
            obstacles.push(model.collision.at(player.position, player.heading));
            let scene = crate::collision::CollisionScene {
                world: &world,
                instances: &obstacles,
            };
            let Some(ped) = find_vehicle_exit(&scene, &candidate, &model.collision, model.bounds)
            else {
                continue;
            };
            assert!(traffic.enter_nearest(&world, &model.collision, &[], ped.feet, &mut player));
            assert_eq!(player.position, candidate.position);
            assert!(traffic.cars.iter().any(|v| v.car.position == old_position));
            let obstacles = traffic.colliders(&model.collision);
            let scene = crate::collision::CollisionScene {
                world: &world,
                instances: &obstacles,
            };
            let exited =
                find_vehicle_exit(&scene, &player, &model.collision, model.bounds).unwrap();
            assert!(exited.grounded);
            assert!(exited.has_clear_body(&scene));
            println!(
                "Native traffic entry {:?}, exit {:?}, previous car {:?}",
                player.position, exited.feet, old_position
            );
            verified = true;
            break;
        }
        assert!(verified, "no native traffic car had an accessible door");
    }

    #[test]
    fn vehicle_exit_uses_clear_side_and_rejects_two_blocked_doors() {
        use crate::collision::{test_box_collider, CollisionWorld, Triangle};
        let tri = |vertices| Triangle {
            vertices,
            surface: 0,
            model_id: 1,
        };
        let mut triangles = vec![
            tri([[-10.0, -10.0, 0.0], [10.0, -10.0, 0.0], [10.0, 10.0, 0.0]]),
            tri([[-10.0, -10.0, 0.0], [10.0, 10.0, 0.0], [-10.0, 10.0, 0.0]]),
        ];
        let bounds = [[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]];
        let collider = test_box_collider(bounds);
        let car = crate::driving::Car::new([0.0, 0.0, 0.015], 0.0);
        let open = CollisionWorld::from_triangles(triangles.clone());
        assert!(
            find_vehicle_exit(&open, &car, &collider, bounds)
                .unwrap()
                .feet[0]
                > 1.3
        );
        let nearby = [
            collider.at([2.3, 0.0, 0.015], 0.0),
            collider.at([-2.3, 0.0, 0.015], 0.0),
        ];
        let scene = crate::collision::CollisionScene {
            world: &open,
            instances: &nearby[..1],
        };
        assert!(
            find_vehicle_exit(&scene, &car, &collider, bounds)
                .unwrap()
                .feet[0]
                < -1.3
        );
        let scene = crate::collision::CollisionScene {
            world: &open,
            instances: &nearby,
        };
        assert!(find_vehicle_exit(&scene, &car, &collider, bounds).is_none());
        for x in [1.4, -1.4] {
            triangles.extend([
                tri([[x, -10.0, 0.0], [x, 10.0, 0.0], [x, 10.0, 5.0]]),
                tri([[x, -10.0, 0.0], [x, 10.0, 5.0], [x, -10.0, 5.0]]),
            ]);
            let world = CollisionWorld::from_triangles(triangles.clone());
            let exit = find_vehicle_exit(&world, &car, &collider, bounds);
            if x > 0.0 {
                assert!(exit.unwrap().feet[0] < -1.3);
            } else {
                assert!(exit.is_none());
            }
        }
    }
    #[test]
    fn vehicle_camera_smooths_turns_across_angle_wrap() {
        let world = crate::collision::CollisionWorld::from_triangles(Vec::new());
        let mut car = crate::driving::Car::new([0.0; 3], -3.1);
        let mut chase = VehicleCamera::new(3.1);
        chase.update(&car, &world, 1.0 / 60.0);
        assert!(chase.yaw > 3.1, "must take short path through PI");
        car.heading = 1.0;
        chase = VehicleCamera::new(0.0);
        let frame = chase.update(&car, &world, 1.0 / 60.0);
        assert!(frame.yaw > 0.0 && frame.yaw < car.heading);
    }

    #[test]
    fn vehicle_camera_retracts_before_near_wall_then_recovers_smoothly() {
        use crate::collision::{CollisionWorld, Triangle};
        let wall = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-10.0, -0.8, 0.0], [10.0, -0.8, 0.0], [10.0, -0.8, 5.0]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-10.0, -0.8, 0.0], [10.0, -0.8, 5.0], [-10.0, -0.8, 5.0]],
                surface: 0,
                model_id: 1,
            },
        ]);
        let car = crate::driving::Car::new([0.0; 3], 0.0);
        let mut chase = VehicleCamera::new(0.0);
        let frame = chase.update(&car, &wall, 1.0 / 60.0);
        assert!(frame.pos[1] > -0.8);
        let blocked = chase.distance;
        let clear = CollisionWorld::from_triangles(Vec::new());
        chase.update(&car, &clear, 1.0 / 60.0);
        assert!(chase.distance > blocked && chase.distance < 6.5);
    }

    fn corner_obstacle(
        target: [f32; 3],
        yaw: f32,
        pitch: f32,
        distance: f32,
    ) -> crate::collision::CollisionWorld {
        use crate::collision::{CollisionWorld, Triangle};
        let (s, c) = yaw.sin_cos();
        let (sp, cp) = pitch.sin_cos();
        let backward = [-s * cp, -c * cp, -sp];
        let right = [c, -s, 0.0];
        let up = [-sp * s, -sp * c, cp];
        let point = |x: f32, y: f32| {
            std::array::from_fn(|i| target[i] + backward[i] * distance + right[i] * x + up[i] * y)
        };
        let p = [
            point(0.36, 0.20),
            point(0.46, 0.20),
            point(0.46, 0.30),
            point(0.36, 0.30),
        ];
        CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [p[0], p[1], p[2]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [p[0], p[2], p[3]],
                surface: 0,
                model_id: 1,
            },
        ])
    }

    #[test]
    fn camera_corners_detect_obstacles_missed_by_center_at_rotated_angles() {
        let target = [10.0, 20.0, 3.0];
        for yaw in [0.0f32, 0.7, -2.4] {
            for pitch in [-0.3f32, 0.0, 0.35] {
                let world = corner_obstacle(target, yaw, pitch, 2.0);
                let (s, c) = yaw.sin_cos();
                let (sp, cp) = pitch.sin_cos();
                assert!(world
                    .raycast(target, [-s * cp, -c * cp, -sp], 4.25)
                    .is_none());
                let distance = camera_clearance(&world, target, yaw, pitch, 4.0, 0.25);
                assert!(
                    (distance - 1.75).abs() < 0.0001,
                    "yaw={yaw}, pitch={pitch}: {distance}"
                );
            }
        }
    }

    #[test]
    fn both_follow_cameras_retract_for_edge_contacts_and_clear_scenes_stay_unchanged() {
        let camera = Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        };
        let wall = corner_obstacle([0.0, 0.0, 1.1], 0.0, 0.0, 2.0);
        let walking = follow_camera(&camera, [0.0; 3], &wall);
        assert!((walking.pos[1] + 1.75).abs() < 0.0001);
        let car = crate::driving::Car::new([0.0; 3], 0.0);
        let wall = corner_obstacle([0.0, 0.0, 1.05], 0.0, -0.25, 2.0);
        let mut chase = VehicleCamera::new(0.0);
        chase.update(&car, &wall, 1.0 / 60.0);
        assert!((chase.distance - 1.65).abs() < 0.0001);
        let clear = crate::collision::CollisionWorld::from_triangles(Vec::new());
        for yaw in [0.0, 1.5] {
            assert_eq!(
                camera_clearance(&clear, [0.0; 3], yaw, -0.25, 7.7, 0.35),
                7.7
            );
        }
    }

    #[test]
    fn follow_camera_stays_in_front_of_wall_and_restores_full_distance() {
        use crate::collision::{CollisionWorld, Triangle};
        let camera = Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        };
        let wall = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-10.0, -2.0, 0.0], [10.0, -2.0, 0.0], [10.0, -2.0, 5.0]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-10.0, -2.0, 0.0], [10.0, -2.0, 5.0], [-10.0, -2.0, 5.0]],
                surface: 0,
                model_id: 1,
            },
        ]);
        let blocked = follow_camera(&camera, [0.0; 3], &wall);
        assert!((blocked.pos[1] + 1.75).abs() < 1e-5);
        assert!((blocked.pos[2] - 1.1).abs() < 1e-5);
        let clear = CollisionWorld::from_triangles(Vec::new());
        let unblocked = follow_camera(&camera, [0.0; 3], &clear);
        assert!((unblocked.pos[1] + 4.0).abs() < 1e-5);
        // A wall immediately behind the target must not be crossed by an
        // artificial minimum camera distance.
        let close = Camera {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
        };
        let blocked = follow_camera(&close, [0.0, -1.9, 0.0], &wall);
        assert!(blocked.pos[1] > -2.0);
    }
    #[test]
    fn alpha_blend_preserves_background_and_composes_material_color() {
        assert_eq!(blend_pixels(0xff0000, 0x0000ff, 0.0), 0x0000ff);
        assert_eq!(blend_pixels(0xff0000, 0x0000ff, 1.0), 0xff0000);
        assert_eq!(blend_pixels(0xff0000, 0x0000ff, 0.5), 0x800080);
    }
    #[test]
    fn clips_triangle_crossing_near_plane() {
        let t = [
            Vertex {
                p: [0.0, 0.0, 0.1],
                color: [1.0; 3],
                uv: [0.0, 0.0],
            },
            Vertex {
                p: [1.0, 0.0, 2.0],
                color: [1.0; 3],
                uv: [1.0, 0.0],
            },
            Vertex {
                p: [0.0, 1.0, 2.0],
                color: [1.0; 3],
                uv: [0.0, 1.0],
            },
        ];
        let c = clip_near(&t, 0.5);
        assert_eq!(c.len(), 4);
        assert!(c.iter().all(|p| p.p[2] >= 0.4999));
    }
}
