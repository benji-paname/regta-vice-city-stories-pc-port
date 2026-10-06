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
/// Camera follows a feet position, stopping before an original static obstacle.
pub fn follow_camera(
    camera: &Camera,
    feet: [f32; 3],
    collisions: &crate::collision::CollisionWorld,
) -> Camera {
    let target = [feet[0], feet[1], feet[2] + 1.1];
    let (s, c) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    let backwards = [-s * cp, -c * cp, -sp];
    let distance = collisions
        .raycast(target, backwards, 4.25)
        .map_or(4.0, |hit| (hit.distance - 0.25).clamp(0.2, 4.0));
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
        if let Some((character, character_textures)) = character {
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
                let clipped = clip_near(&points, 0.5);
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
        if let Some((vehicle, vehicle_textures)) = vehicle {
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
                let clipped = clip_near(&points, 0.5);
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
            let clipped = clip_near(&points, 0.5);
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
                WIDTH as f32 / 2.0 + v.p[0] * 650.0 / v.p[2],
                HEIGHT as f32 / 2.0 - v.p[1] * 650.0 / v.p[2],
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
    pub smoke: bool,
    pub initial: usize,
    pub start_walking: bool,
    pub software: bool,
    pub capture: Option<PathBuf>,
    pub headless: bool,
    pub initial_camera: Option<Camera>,
    pub third_person: bool,
    pub animation: Option<String>,
    pub animation_time: Option<f32>,
    pub simulate_seconds: f32,
}
pub fn viewer(
    levels: &[Level],
    collisions: &mut crate::collision::CollisionWorld,
    character: &crate::character::Character,
    vehicle: &crate::vehicle::VehicleModel,
    options: ViewerOptions,
) -> Result<serde_json::Value> {
    let ViewerOptions {
        smoke,
        initial,
        start_walking,
        software,
        capture,
        headless,
        initial_camera,
        mut third_person,
        animation,
        animation_time,
        simulate_seconds,
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
    if let Some(gpu) = &mut gpu {
        gpu.offscreen = headless;
        gpu.set_character_textures(&character.textures)?;
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
        crate::player::Walker::from_camera(collisions, &camera)
    } else {
        None
    };
    if let Some(walker) = &walker {
        camera.pos = walker.eye();
    }
    if simulate_seconds > 0.0 {
        let walking = walker.as_mut().ok_or("Simulation requires walking mode")?;
        let (s, c) = camera.yaw.sin_cos();
        for _ in 0..(simulate_seconds * 120.0).ceil() as usize {
            walking.step(
                collisions,
                [s, c],
                if animation.as_deref() == Some("run") {
                    4.5
                } else {
                    1.8
                },
                false,
                1.0 / 120.0,
            );
        }
        camera.pos = walking.eye();
    }
    let mut index = initial;
    let mut previous = Instant::now();
    let mut ticks = 0;
    let mut textures = true;
    let mut show_help = true;
    let mut physics_time = 0.0;
    let mut pending_jump = false;
    let mut render_seconds = 0.0;
    let mut submitted_triangles = 0;
    let mut animation_clock = 0.0;
    let mut last_clip = "idle";
    let mut heading = camera.yaw;
    let mut destination = initial;
    let mut last_render_camera = camera;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = now.duration_since(previous).as_secs_f32().min(0.1);
        previous = now;
        if window.is_key_pressed(Key::T, minifb::KeyRepeat::No) {
            textures = !textures;
        }
        if window.is_key_pressed(Key::F1, minifb::KeyRepeat::No) {
            show_help = !show_help;
        }
        if window.is_key_pressed(Key::F2, minifb::KeyRepeat::No) {
            third_person = !third_person;
        }
        for (target, key) in [Key::Key1, Key::Key2, Key::Key3, Key::Key4]
            .iter()
            .enumerate()
        {
            if window.is_key_pressed(*key, minifb::KeyRepeat::No) {
                destination = target;
                index = if target == 3 { 1 } else { target };
                crate::scene::select_collisions(collisions, index);
                let walking = walker.is_some();
                camera = if target == 3 {
                    Camera::military(walking)
                } else if walking {
                    Camera::walking_spawn(index)
                } else {
                    Camera::for_zone(index)
                };
                walker = if walking {
                    crate::player::Walker::from_camera(collisions, &camera)
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
        }
        if window.is_key_pressed(Key::G, minifb::KeyRepeat::No) {
            if walker.is_some() {
                walker = None;
            } else {
                // The original mainland has a map-wide flight ceiling at z107.5.
                // Start the ground query below it, preserving the native collider.
                if index != 2 {
                    camera.pos[2] = camera.pos[2].min(90.0);
                }
                walker = crate::player::Walker::from_camera(collisions, &camera);
                if let Some(walker) = &walker {
                    camera.pos = walker.eye();
                }
            }
            physics_time = 0.0;
            pending_jump = false;
        }
        if window.is_key_pressed(Key::R, minifb::KeyRepeat::No) {
            let walking = walker.is_some();
            camera = if destination == 3 {
                Camera::military(walking)
            } else if walking {
                Camera::walking_spawn(index)
            } else {
                Camera::for_zone(index)
            };
            walker = if walking {
                crate::player::Walker::from_camera(collisions, &camera)
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
        let forward = (window.is_key_down(Key::W) || window.is_key_down(Key::Z)) as i32
            - window.is_key_down(Key::S) as i32;
        let side = window.is_key_down(Key::D) as i32
            - (window.is_key_down(Key::A) || window.is_key_down(Key::Q)) as i32;
        let (s, c) = camera.yaw.sin_cos();
        let axis = [
            forward as f32 * s + side as f32 * c,
            forward as f32 * c - side as f32 * s,
        ];
        if let Some(walker) = &mut walker {
            pending_jump |= window.is_key_pressed(Key::Space, minifb::KeyRepeat::No);
            let speed = if window.is_key_down(Key::LeftShift) {
                4.5
            } else {
                1.8
            };
            physics_time += dt;
            while physics_time >= 1.0 / 120.0 {
                walker.step(collisions, axis, speed, pending_jump, 1.0 / 120.0);
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
        camera.yaw += (window.is_key_down(Key::Right) as i32 - window.is_key_down(Key::Left) as i32)
            as f32
            * dt
            * 1.4;
        camera.pitch = (camera.pitch
            + (window.is_key_down(Key::Up) as i32 - window.is_key_down(Key::Down) as i32) as f32
                * dt)
            .clamp(-1.4, 1.4);
        let moving = forward != 0 || side != 0;
        if moving && walker.is_some() {
            heading = axis[0].atan2(axis[1]);
        }
        let clip = if moving && walker.as_ref().is_some_and(|w| w.grounded) {
            if window.is_key_down(Key::LeftShift) {
                "run"
            } else {
                "walk"
            }
        } else {
            "idle"
        };
        if clip != last_clip {
            animation_clock = 0.0;
            last_clip = clip;
        }
        animation_clock += dt;
        let show_character = third_person && walker.is_some();
        let posed = if show_character {
            character.triangles(
                animation.as_deref().unwrap_or(clip),
                animation_time.unwrap_or(animation_clock),
            )
        } else {
            Vec::new()
        };
        let character_frame = if show_character {
            walker.as_ref().map(|w| crate::gpu::CharacterFrame {
                triangles: &posed,
                position: w.feet,
                heading,
            })
        } else {
            None
        };
        let render_camera = if show_character {
            follow_camera(&camera, walker.as_ref().unwrap().feet, collisions)
        } else {
            camera
        };
        last_render_camera = render_camera;
        let vehicle_pos = match index {
            1 => [-1005.0, -495.0, 11.0],
            2 => [385.0, 1125.0, -212.0],
            _ => [504.0, -496.0, 10.5],
        };
        let vehicle_frame = crate::gpu::VehicleFrame {
            triangles: &vehicle.triangles,
            position: vehicle_pos,
            heading: -1.1,
        };
        let draw_started = Instant::now();
        if let Some(gpu) = &mut gpu {
            gpu.draw(
                levels,
                index,
                &render_camera,
                textures,
                walker.is_some(),
                show_help,
                character_frame.as_ref(),
                Some(&vehicle_frame),
            )?;
            submitted_triangles = gpu.triangles;
            render_seconds += draw_started.elapsed().as_secs_f64();
            window.update();
        } else {
            frame.draw_scene_all(
                levels,
                index,
                &render_camera,
                textures,
                character_frame.as_ref().map(|f| (f, &character.textures)),
                Some((&vehicle_frame, &vehicle.textures)),
            );
            crate::hud::guide(
                &mut frame.pixels,
                crate::scene::label(index),
                walker.is_some(),
                show_help,
            );
            submitted_triangles = frame.triangles;
            render_seconds += draw_started.elapsed().as_secs_f64();
            window.update_with_buffer(&frame.pixels, WIDTH, HEIGHT)?;
        }
        window.set_title(&format!(
            "VCS Rust | {} | {} | F1 : aide",
            crate::scene::label(index),
            if walker.is_some() {
                "Marche experimentale"
            } else {
                "Camera libre"
            }
        ));
        ticks += 1;
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
        serde_json::json!({"zone":levels[index].stats.name,"scene":crate::scene::label(index),"scene_levels":crate::scene::level_indices(index),"draw_distance":crate::scene::DRAW_DISTANCE,"renderer":if gpu.is_some(){"OpenGL"}else{"software"},"driver":gpu.as_ref().map(|g|&g.driver),"frames":ticks,"submitted_triangles":submitted_triangles,"walking":walker.is_some(),"feet":walker.as_ref().map(|w|w.feet),"grounded":walker.as_ref().map(|w|w.grounded),"third_person":third_person,"animation":animation.as_deref().unwrap_or(last_clip),"animation_time":animation_time.unwrap_or(animation_clock),"simulation_seconds":simulate_seconds,"camera":last_render_camera.pos,"average_draw_ms":render_seconds*1000.0/ticks.max(1) as f64}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
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
