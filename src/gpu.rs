//! Windows OpenGL compatibility renderer. Game assets and logic remain native
//! Rust; the system graphics driver draws the already-decoded world meshes.

use crate::iso::Result;
use crate::leeds::{Instance, Level, Triangle};
use crate::render::{Camera, HEIGHT, WIDTH};
use crate::texture::Texture;
use minifb::Window;
use std::collections::{BTreeMap, HashMap};
use std::ffi::{c_char, c_void, CStr};
use std::io::Write;
use std::path::Path;
use std::ptr::null_mut;

type Handle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct PixelFormat {
    size: u16,
    version: u16,
    flags: u32,
    pixel_type: u8,
    color_bits: u8,
    red_bits: u8,
    red_shift: u8,
    green_bits: u8,
    green_shift: u8,
    blue_bits: u8,
    blue_shift: u8,
    alpha_bits: u8,
    alpha_shift: u8,
    accum_bits: u8,
    accum_red_bits: u8,
    accum_green_bits: u8,
    accum_blue_bits: u8,
    accum_alpha_bits: u8,
    depth_bits: u8,
    stencil_bits: u8,
    aux_buffers: u8,
    layer_type: u8,
    reserved: u8,
    layer_mask: u32,
    visible_mask: u32,
    damage_mask: u32,
}

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    fn GetDC(window: Handle) -> Handle;
    fn ReleaseDC(window: Handle, dc: Handle) -> i32;
    fn GetClientRect(window: Handle, rect: *mut Rect) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn ChoosePixelFormat(dc: Handle, descriptor: *const PixelFormat) -> i32;
    fn GetPixelFormat(dc: Handle) -> i32;
    fn SetPixelFormat(dc: Handle, format: i32, descriptor: *const PixelFormat) -> i32;
    fn SwapBuffers(dc: Handle) -> i32;
}

#[link(name = "opengl32")]
extern "system" {
    fn wglCreateContext(dc: Handle) -> Handle;
    fn wglDeleteContext(context: Handle) -> i32;
    fn wglMakeCurrent(dc: Handle, context: Handle) -> i32;
    fn wglGetProcAddress(name: *const c_char) -> *const c_void;
    fn glGetString(name: u32) -> *const u8;
    fn glGetError() -> u32;
    fn glGetFloatv(name: u32, values: *mut f32);
    fn glViewport(x: i32, y: i32, width: i32, height: i32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn glEnable(capability: u32);
    fn glDisable(capability: u32);
    fn glDepthFunc(function: u32);
    fn glDepthMask(enabled: u8);
    fn glAlphaFunc(function: u32, reference: f32);
    fn glBlendFunc(source: u32, destination: u32);
    fn glShadeModel(mode: u32);
    fn glHint(target: u32, mode: u32);
    fn glFogf(name: u32, value: f32);
    fn glFogfv(name: u32, value: *const f32);
    fn glMatrixMode(mode: u32);
    fn glLoadIdentity();
    fn glLoadMatrixf(matrix: *const f32);
    fn glMultMatrixf(matrix: *const f32);
    fn glPushMatrix();
    fn glPopMatrix();
    fn glFrustum(left: f64, right: f64, bottom: f64, top: f64, near: f64, far: f64);
    fn glOrtho(left: f64, right: f64, bottom: f64, top: f64, near: f64, far: f64);
    fn glBegin(mode: u32);
    fn glEnd();
    fn glColor3f(r: f32, g: f32, b: f32);
    fn glColor4f(r: f32, g: f32, b: f32, a: f32);
    fn glTexCoord2f(u: f32, v: f32);
    fn glVertex2f(x: f32, y: f32);
    fn glVertex3f(x: f32, y: f32, z: f32);
    fn glGenTextures(count: i32, textures: *mut u32);
    fn glDeleteTextures(count: i32, textures: *const u32);
    fn glBindTexture(target: u32, texture: u32);
    fn glTexParameteri(target: u32, name: u32, value: i32);
    fn glTexParameterf(target: u32, name: u32, value: f32);
    fn glTexEnvi(target: u32, name: u32, value: i32);
    fn glPixelStorei(name: u32, value: i32);
    fn glTexImage2D(
        target: u32,
        level: i32,
        internal_format: i32,
        width: i32,
        height: i32,
        border: i32,
        format: u32,
        data_type: u32,
        pixels: *const c_void,
    );
    fn glGenLists(count: i32) -> u32;
    fn glDeleteLists(first: u32, count: i32);
    fn glNewList(list: u32, mode: u32);
    fn glEndList();
    fn glCallList(list: u32);
    fn glReadBuffer(buffer: u32);
    fn glReadPixels(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        format: u32,
        data_type: u32,
        pixels: *mut c_void,
    );
}

const TEXTURE_2D: u32 = 0x0de1;
const DEPTH_TEST: u32 = 0x0b71;
const ALPHA_TEST: u32 = 0x0bc0;
const FOG: u32 = 0x0b60;
const BLEND: u32 = 0x0be2;
const PROJECTION: u32 = 0x1701;
const MODELVIEW: u32 = 0x1700;
const LINES: u32 = 0x0001;
const TRIANGLES: u32 = 0x0004;
const QUADS: u32 = 0x0007;

struct Batch {
    texture: u32,
    list: u32,
    triangles: usize,
}

#[derive(Default)]
struct GpuLevel {
    meshes: HashMap<u16, Vec<Batch>>,
}

/// A posed character mesh in local coordinates, anchored at its world-space feet.
pub struct CharacterFrame<'a> {
    pub triangles: &'a [Triangle],
    pub position: [f32; 3],
    /// Zero faces local +Y; positive headings turn toward world +X.
    pub heading: f32,
}

/// An assembled vehicle mesh in local coordinates, placed in world space.
pub struct VehicleFrame<'a> {
    pub triangles: &'a [Triangle],
    pub position: [f32; 3],
    pub heading: f32,
    pub health: f32,
    pub burning: bool,
    pub exploded: bool,
    pub is_police: bool,
    pub strobe_timer: f32,
    pub headlights: bool,
}

impl<'a> VehicleFrame<'a> {
    #[allow(dead_code)]
    pub fn new(triangles: &'a [Triangle], position: [f32; 3], heading: f32) -> Self {
        Self {
            triangles,
            position,
            heading,
            health: 1000.0,
            burning: false,
            exploded: false,
            is_police: false,
            strobe_timer: 0.0,
            headlights: true,
        }
    }
}

/// Must be dropped while its minifb window still exists, on the creating thread.
pub struct GpuRenderer {
    window: Handle,
    dc: Handle,
    context: Handle,
    levels: Vec<GpuLevel>,
    texture_handles: Vec<u32>,
    list_handles: Vec<u32>,
    character_textures: HashMap<u16, u32>,
    pub vehicle_textures: HashMap<u16, u32>,
    hud_handles: [u32; 3],
    anisotropy: f32,
    scene_instances: [Vec<(usize, usize)>; 3],
    pub driver: String,
    /// Triangles submitted to the graphics driver in the latest frame.
    pub triangles: usize,
    pub textured_triangles: usize,
    pub instances: usize,
    /// Keep the rendered image in the back buffer for hidden-window verification.
    pub offscreen: bool,
}

impl GpuRenderer {
    pub fn new(window: &Window, levels: &[Level]) -> Result<Self> {
        let handle = window.get_window_handle();
        let mut renderer = Self {
            window: handle,
            dc: null_mut(),
            context: null_mut(),
            levels: Vec::new(),
            texture_handles: Vec::new(),
            list_handles: Vec::new(),
            character_textures: HashMap::new(),
            vehicle_textures: HashMap::new(),
            hud_handles: [0; 3],
            anisotropy: 1.0,
            scene_instances: std::array::from_fn(|zone| {
                crate::scene::unique_instances(levels, zone)
                    .into_iter()
                    .filter(|&(level, instance)| {
                        levels[level].visible(&levels[level].instances[instance], 12)
                    })
                    .collect()
            }),
            driver: String::new(),
            triangles: 0,
            textured_triangles: 0,
            instances: 0,
            offscreen: false,
        };
        unsafe {
            renderer.dc = GetDC(handle);
            if renderer.dc.is_null() {
                return Err("OpenGL: impossible d'obtenir la surface Windows".into());
            }
            let descriptor = PixelFormat {
                size: std::mem::size_of::<PixelFormat>() as u16,
                version: 1,
                flags: 0x00000001 | 0x00000004 | 0x00000020,
                color_bits: 32,
                alpha_bits: 8,
                depth_bits: 24,
                stencil_bits: 8,
                ..PixelFormat::default()
            };
            if GetPixelFormat(renderer.dc) == 0 {
                let format = ChoosePixelFormat(renderer.dc, &descriptor);
                if format == 0 || SetPixelFormat(renderer.dc, format, &descriptor) == 0 {
                    return Err("OpenGL: format de rendu indisponible".into());
                }
            }
            renderer.context = wglCreateContext(renderer.dc);
            if renderer.context.is_null() || wglMakeCurrent(renderer.dc, renderer.context) == 0 {
                return Err("OpenGL: creation du contexte impossible".into());
            }
            let vendor = gl_string(0x1f00);
            let device = gl_string(0x1f01);
            let version = gl_string(0x1f02);
            renderer.driver = format!("{vendor} | {device} | OpenGL {version}");
            if device == "GDI Generic" || version.is_empty() {
                return Err("OpenGL: aucun pilote graphique accelere disponible".into());
            }
            let extension = gl_string(0x1f03);
            let anisotropy = if extension.contains("GL_EXT_texture_filter_anisotropic") {
                let mut maximum = 1.0;
                glGetFloatv(0x84ff, &mut maximum);
                maximum.min(8.0).max(1.0)
            } else {
                1.0
            };
            renderer.anisotropy = anisotropy;
            let swap = wglGetProcAddress(b"wglSwapIntervalEXT\0".as_ptr().cast());
            let address = swap as usize;
            if address > 3 && address != usize::MAX {
                let swap_interval: unsafe extern "system" fn(i32) -> i32 =
                    std::mem::transmute(swap);
                swap_interval(1);
            }
            glPixelStorei(0x0cf5, 1); // UNPACK_ALIGNMENT
            glEnable(DEPTH_TEST);
            glDepthFunc(0x0201); // LESS
            glAlphaFunc(0x0206, 0.5); // GEQUAL
            glShadeModel(0x1d01); // SMOOTH
            glHint(0x0c50, 0x1102); // PERSPECTIVE_CORRECTION_HINT, NICEST
            glTexEnvi(0x2300, 0x2200, 0x2100); // TEXTURE_ENV, MODE, MODULATE
            glFogf(0x0b65, 0x2601 as f32); // FOG_MODE, LINEAR
            glFogf(0x0b63, 650.0);
            glFogf(0x0b64, 4500.0);
            glFogfv(0x0b66, [0.58, 0.72, 0.80, 1.0].as_ptr());
            for level in levels {
                renderer.upload_level(level, anisotropy)?;
            }
            check_error("initialisation")?;
        }
        Ok(renderer)
    }

    unsafe fn upload_level(&mut self, level: &Level, anisotropy: f32) -> Result<()> {
        let mut textures = HashMap::new();
        for (&id, texture) in &level.textures {
            let handle = self.upload_texture(texture, anisotropy)?;
            textures.insert(id, handle);
        }
        let mut uploaded = GpuLevel::default();
        for (&resource, mesh) in &level.meshes {
            let mut groups = BTreeMap::<u16, Vec<_>>::new();
            for triangle in &mesh.triangles {
                groups
                    .entry(triangle.texture_id)
                    .or_default()
                    .push(triangle);
            }
            let mut batches = Vec::with_capacity(groups.len());
            for (texture_id, triangles) in groups {
                let list = glGenLists(1);
                if list == 0 {
                    return Err("OpenGL: allocation de maillage impossible".into());
                }
                self.list_handles.push(list);
                glNewList(list, 0x1300); // COMPILE
                glBegin(0x0004); // TRIANGLES
                for triangle in &triangles {
                    for vertex in &triangle.vertices {
                        glColor3f(vertex.color[0], vertex.color[1], vertex.color[2]);
                        glTexCoord2f(vertex.uv[0], vertex.uv[1]);
                        glVertex3f(vertex.p[0], vertex.p[1], vertex.p[2]);
                    }
                }
                glEnd();
                glEndList();
                batches.push(Batch {
                    texture: textures.get(&texture_id).copied().unwrap_or(0),
                    list,
                    triangles: triangles.len(),
                });
            }
            uploaded.meshes.insert(resource, batches);
        }
        check_error("chargement des maillages")?;
        self.levels.push(uploaded);
        Ok(())
    }

    unsafe fn upload_texture(&mut self, texture: &Texture, anisotropy: f32) -> Result<u32> {
        let mut handle = 0;
        glGenTextures(1, &mut handle);
        if handle == 0 {
            return Err("OpenGL: allocation de texture impossible".into());
        }
        self.texture_handles.push(handle);
        glBindTexture(TEXTURE_2D, handle);
        glTexParameteri(TEXTURE_2D, 0x2801, 0x2703); // MIN, LINEAR_MIPMAP_LINEAR
        glTexParameteri(TEXTURE_2D, 0x2800, 0x2601); // MAG, LINEAR
        glTexParameteri(TEXTURE_2D, 0x2802, 0x2901); // WRAP_S, REPEAT
        glTexParameteri(TEXTURE_2D, 0x2803, 0x2901); // WRAP_T, REPEAT
        if anisotropy > 1.0 {
            glTexParameterf(TEXTURE_2D, 0x84fe, anisotropy);
        }
        for mip in 0..texture.mip_levels() {
            let (width, height, rgba) = texture.mip(mip).ok_or("Missing texture mip")?;
            glTexImage2D(
                TEXTURE_2D,
                mip as i32,
                0x8058,
                width as i32,
                height as i32,
                0,
                0x1908,
                0x1401,
                rgba.as_ptr().cast(),
            );
        }
        check_error("chargement des textures")?;
        Ok(handle)
    }

    /// Upload each character material once. Character IDs use their own namespace.
    pub fn set_character_textures(&mut self, textures: &HashMap<u16, Texture>) -> Result<()> {
        unsafe {
            if wglMakeCurrent(self.dc, self.context) == 0 {
                return Err("OpenGL: contexte de rendu perdu".into());
            }
            for (&id, texture) in textures {
                if !self.character_textures.contains_key(&id) {
                    let handle = self.upload_texture(texture, self.anisotropy)?;
                    self.character_textures.insert(id, handle);
                }
            }
        }
        Ok(())
    }

    pub fn set_vehicle_textures(&mut self, textures: &HashMap<u16, Texture>) -> Result<()> {
        unsafe {
            if wglMakeCurrent(self.dc, self.context) == 0 {
                return Err("OpenGL: contexte de rendu perdu".into());
            }
            for (&id, texture) in textures {
                if !self.vehicle_textures.contains_key(&id) {
                    let handle = self.upload_texture(texture, self.anisotropy)?;
                    self.vehicle_textures.insert(id, handle);
                }
            }
        }
        Ok(())
    }

    pub fn draw(
        &mut self,
        levels: &[Level],
        zone: usize,
        camera: &Camera,
        textures: bool,
        walking: bool,
        driving: bool,
        show_help: bool,
        characters: &[CharacterFrame<'_>],
        vehicles: &[VehicleFrame<'_>],
        hud: &[crate::hud::Layer],
        lights: &[crate::traffic::TrafficLight],
        weapons: Option<&crate::weapons::WeaponArsenal>,
        mission: Option<&crate::script::MissionScriptSystem>,
        global_time: f32,
    ) -> Result<()> {
        if zone >= levels.len() || zone >= self.scene_instances.len() {
            return Err("Invalid GPU zone".into());
        }
        let (width, height) = self.dimensions()?;
        self.triangles = 0;
        self.textured_triangles = 0;
        self.instances = 0;
        unsafe {
            if wglMakeCurrent(self.dc, self.context) == 0 {
                return Err("OpenGL: contexte de rendu perdu".into());
            }
            let hour = (12.0 + global_time * (24.0 / 600.0)).rem_euclid(24.0);
            let (_, mid, _, _fog) = sky_atmosphere(hour);
            glViewport(0, 0, width, height);
            glDepthMask(1);
            glClearColor(mid[0], mid[1], mid[2], 1.0);
            glClear(0x00004000 | 0x00000100); // COLOR, DEPTH
            ortho();
            background(hour);
            glMatrixMode(PROJECTION);
            glLoadIdentity();
            let near = crate::render::CAMERA_NEAR as f64;
            let right = near * WIDTH as f64 / (2.0 * crate::render::CAMERA_FOCAL as f64);
            let top = near * HEIGHT as f64 / (2.0 * crate::render::CAMERA_FOCAL as f64);
            glFrustum(
                -right,
                right,
                -top,
                top,
                near,
                (crate::scene::DRAW_DISTANCE + 1000.0) as f64,
            );
            glMatrixMode(MODELVIEW);
            glLoadMatrixf(view_matrix(camera).as_ptr());
            glEnable(DEPTH_TEST);
            glEnable(ALPHA_TEST);
            glAlphaFunc(0x0206, 0.5); // GEQUAL: opaque surfaces and cutouts.
            glEnable(FOG);
            glDisable(BLEND);
            glDisable(0x0b44); // CULL_FACE: PS2 map strips include two-sided material usage.
            let mut visible = Vec::new();
            for &(level_index, instance_index) in &self.scene_instances[zone] {
                let level = levels.get(level_index).ok_or("Invalid GPU scene level")?;
                let gpu = self
                    .levels
                    .get(level_index)
                    .ok_or("GPU scene level was not uploaded")?;
                let instance = level
                    .instances
                    .get(instance_index)
                    .ok_or("Invalid cached GPU instance")?;
                if !crate::scene::instance_in_view(instance, camera) {
                    continue;
                }
                let Some(batches) = gpu.meshes.get(&instance.resource) else {
                    continue;
                };
                visible.push((instance, batches));
            }
            // Both outdoor archives share one transparent ordering. Sorting each
            // archive separately would put some near surfaces behind distant ones.
            visible.sort_by(|(a, _), (b, _)| instance_order(a, b, camera));
            // Actors and vehicles must write depth before translucent world
            // surfaces, including water. Opaque world geometry still occludes
            // them normally through the depth test below.
            for character in characters {
                let (triangles, textured) =
                    draw_character(character, &self.character_textures, textures);
                self.triangles += triangles;
                self.textured_triangles += textured;
            }
            for vehicle in vehicles {
                let (triangles, textured) = draw_vehicle(vehicle, &self.vehicle_textures, textures);
                self.triangles += triangles;
                self.textured_triangles += textured;
            }
            draw_traffic_lights(lights, global_time);
            draw_vehicle_effects(vehicles, global_time, hour);
            if let Some(w) = weapons {
                draw_weapons_3d(w);
            }
            if let Some(m) = mission {
                draw_mission_corona(m.marker_position(), m.marker_pulse);
            }
            let mut blending = false;
            for (instance, batches) in visible {
                if crate::scene::is_blended(instance) && !blending {
                    glEnable(BLEND);
                    glBlendFunc(0x0302, 0x0303); // SRC_ALPHA, ONE_MINUS_SRC_ALPHA
                    glDepthMask(0);
                    glAlphaFunc(0x0206, 0.01);
                    blending = true;
                }
                self.instances += 1;
                glPushMatrix();
                let mut matrix = instance.matrix;
                for index in [3, 7, 11] {
                    matrix[index] = 0.0;
                }
                matrix[15] = 1.0;
                glMultMatrixf(matrix.as_ptr());
                for batch in batches {
                    if textures && batch.texture != 0 {
                        glEnable(TEXTURE_2D);
                        glBindTexture(TEXTURE_2D, batch.texture);
                        self.textured_triangles += batch.triangles;
                    } else {
                        glDisable(TEXTURE_2D);
                    }
                    glCallList(batch.list);
                    self.triangles += batch.triangles;
                }
                glPopMatrix();
            }
            glDepthMask(1);
            glDisable(BLEND);
            ortho();
            self.draw_hud(hud);
            if let Some(w) = weapons {
                draw_crosshair_hud(w.crosshair_kind());
            }
            if let Some(m) = mission {
                draw_mission_hud(m.banner_text(), m.objective_text());
            }
            draw_guide(crate::scene::label(zone), walking, driving, show_help);
            check_error("rendu")?;
            if !self.offscreen && SwapBuffers(self.dc) == 0 {
                return Err("OpenGL: affichage de l'image impossible".into());
            }
        }
        Ok(())
    }

    unsafe fn draw_hud(&mut self, layers: &[crate::hud::Layer]) {
        glEnable(BLEND);
        glBlendFunc(0x0302, 0x0303);
        glEnable(TEXTURE_2D);
        glColor4f(1., 1., 1., 1.);
        for (i, layer) in layers.iter().take(3).enumerate() {
            if self.hud_handles[i] == 0 {
                glGenTextures(1, &mut self.hud_handles[i]);
                self.texture_handles.push(self.hud_handles[i]);
            }
            glBindTexture(TEXTURE_2D, self.hud_handles[i]);
            glTexParameteri(TEXTURE_2D, 0x2801, 0x2601); // LINEAR, no mip chain
            glTexParameteri(TEXTURE_2D, 0x2800, 0x2601);
            glTexParameteri(TEXTURE_2D, 0x2802, 0x812f); // CLAMP_TO_EDGE
            glTexParameteri(TEXTURE_2D, 0x2803, 0x812f);
            glTexImage2D(
                TEXTURE_2D,
                0,
                0x1908,
                layer.width as i32,
                layer.height as i32,
                0,
                0x1908,
                0x1401,
                layer.rgba.as_ptr().cast(),
            );
            let x = layer.x as f32;
            let y = layer.y as f32;
            let w = layer.width as f32;
            let h = layer.height as f32;
            glBegin(QUADS);
            glTexCoord2f(0., 0.);
            glVertex2f(x, y);
            glTexCoord2f(1., 0.);
            glVertex2f(x + w, y);
            glTexCoord2f(1., 1.);
            glVertex2f(x + w, y + h);
            glTexCoord2f(0., 1.);
            glVertex2f(x, y + h);
            glEnd();
        }
        glDisable(TEXTURE_2D);
        glDisable(BLEND);
    }

    fn dimensions(&self) -> Result<(i32, i32)> {
        let mut rect = Rect::default();
        if unsafe { GetClientRect(self.window, &mut rect) } == 0 {
            return Err("OpenGL: surface Windows indisponible".into());
        }
        Ok((
            (rect.right - rect.left).max(1),
            (rect.bottom - rect.top).max(1),
        ))
    }

    /// Capture the completed driver frame for visual verification.
    pub fn write_bmp(&self, path: &Path) -> Result<()> {
        let (width, height) = self.dimensions()?;
        let (width, height) = (width as usize, height as usize);
        let mut rgb = vec![0u8; width * height * 3];
        unsafe {
            if wglMakeCurrent(self.dc, self.context) == 0 {
                return Err("OpenGL capture failed".into());
            }
            glPixelStorei(0x0d05, 1); // PACK_ALIGNMENT
            glReadBuffer(if self.offscreen { 0x0405 } else { 0x0404 }); // BACK / FRONT
            glReadPixels(
                0,
                0,
                width as i32,
                height as i32,
                0x1907,
                0x1401,
                rgb.as_mut_ptr().cast(),
            );
            glReadBuffer(0x0405); // BACK
            check_error("capture")?;
        }
        let stride = (width * 3 + 3) & !3;
        let size = 54 + stride * height;
        let mut header = [0u8; 54];
        header[0..2].copy_from_slice(b"BM");
        header[2..6].copy_from_slice(&(size as u32).to_le_bytes());
        header[10..14].copy_from_slice(&54u32.to_le_bytes());
        header[14..18].copy_from_slice(&40u32.to_le_bytes());
        header[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        header[22..26].copy_from_slice(&(height as i32).to_le_bytes());
        header[26..28].copy_from_slice(&1u16.to_le_bytes());
        header[28..30].copy_from_slice(&24u16.to_le_bytes());
        let mut file = std::fs::File::create(path)?;
        file.write_all(&header)?;
        for y in 0..height {
            let mut row = vec![0; stride];
            for x in 0..width {
                let source = (y * width + x) * 3;
                row[x * 3..x * 3 + 3].copy_from_slice(&[
                    rgb[source + 2],
                    rgb[source + 1],
                    rgb[source],
                ]);
            }
            file.write_all(&row)?;
        }
        Ok(())
    }
}

impl Drop for GpuRenderer {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                if wglMakeCurrent(self.dc, self.context) != 0 {
                    for &list in &self.list_handles {
                        glDeleteLists(list, 1);
                    }
                    if !self.texture_handles.is_empty() {
                        glDeleteTextures(
                            self.texture_handles.len() as i32,
                            self.texture_handles.as_ptr(),
                        );
                    }
                    wglMakeCurrent(null_mut(), null_mut());
                }
                wglDeleteContext(self.context);
            }
            if !self.dc.is_null() {
                ReleaseDC(self.window, self.dc);
            }
        }
    }
}

unsafe fn gl_string(name: u32) -> String {
    let value = glGetString(name);
    if value.is_null() {
        String::new()
    } else {
        CStr::from_ptr(value.cast()).to_string_lossy().into_owned()
    }
}

unsafe fn check_error(operation: &str) -> Result<()> {
    let error = glGetError();
    if error == 0 {
        Ok(())
    } else {
        Err(format!("OpenGL: erreur {error:#x} pendant {operation}").into())
    }
}

fn view_matrix(camera: &Camera) -> [f32; 16] {
    let (s, c) = camera.yaw.sin_cos();
    let (sp, cp) = camera.pitch.sin_cos();
    let x = [c, -s, 0.0];
    let y = [-sp * s, -sp * c, cp];
    let z = [-cp * s, -cp * c, -sp];
    let dot = |row: [f32; 3]| row.iter().zip(camera.pos).map(|(a, b)| a * b).sum::<f32>();
    [
        x[0],
        y[0],
        z[0],
        0.0,
        x[1],
        y[1],
        z[1],
        0.0,
        x[2],
        y[2],
        z[2],
        0.0,
        -dot(x),
        -dot(y),
        -dot(z),
        1.0,
    ]
}

fn instance_distance_squared(instance: &Instance, camera: &Camera) -> f32 {
    let center = if instance.sphere[..3].iter().all(|v| v.is_finite()) {
        [instance.sphere[0], instance.sphere[1], instance.sphere[2]]
    } else {
        [
            instance.matrix[12],
            instance.matrix[13],
            instance.matrix[14],
        ]
    };
    center
        .iter()
        .zip(camera.pos)
        .map(|(p, eye)| (p - eye).powi(2))
        .sum()
}

fn instance_order(a: &Instance, b: &Instance, camera: &Camera) -> std::cmp::Ordering {
    let a_blended = crate::scene::is_blended(a);
    let b_blended = crate::scene::is_blended(b);
    a_blended.cmp(&b_blended).then_with(|| {
        if a_blended && b_blended {
            instance_distance_squared(b, camera).total_cmp(&instance_distance_squared(a, camera))
        } else {
            std::cmp::Ordering::Equal
        }
    })
}

fn character_matrix(position: [f32; 3], heading: f32) -> [f32; 16] {
    let (s, c) = heading.sin_cos();
    [
        c,
        -s,
        0.0,
        0.0,
        s,
        c,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        position[0],
        position[1],
        position[2],
        1.0,
    ]
}

unsafe fn draw_character(
    frame: &CharacterFrame<'_>,
    material_textures: &HashMap<u16, u32>,
    textures: bool,
) -> (usize, usize) {
    glPushMatrix();
    glMultMatrixf(character_matrix(frame.position, frame.heading).as_ptr());
    let mut groups = BTreeMap::<u16, Vec<&Triangle>>::new();
    for triangle in frame.triangles {
        groups
            .entry(triangle.texture_id)
            .or_default()
            .push(triangle);
    }
    let mut textured = 0;
    for (material, triangles) in groups {
        if let Some(&handle) = material_textures.get(&material).filter(|_| textures) {
            glEnable(TEXTURE_2D);
            glBindTexture(TEXTURE_2D, handle);
            textured += triangles.len();
        } else {
            glDisable(TEXTURE_2D);
        }
        glBegin(0x0004); // Dynamic posed triangles; world meshes retain display lists.
        for triangle in triangles {
            for vertex in &triangle.vertices {
                glColor3f(vertex.color[0], vertex.color[1], vertex.color[2]);
                glTexCoord2f(vertex.uv[0], vertex.uv[1]);
                glVertex3f(vertex.p[0], vertex.p[1], vertex.p[2]);
            }
        }
        glEnd();
    }
    glPopMatrix();
    (frame.triangles.len(), textured)
}

unsafe fn draw_vehicle(
    frame: &VehicleFrame<'_>,
    material_textures: &HashMap<u16, u32>,
    textures: bool,
) -> (usize, usize) {
    glPushMatrix();
    glMultMatrixf(character_matrix(frame.position, frame.heading).as_ptr());
    let mut textured = 0;
    let mut active_texture = None;
    let mut active_textured = false;
    for triangle in frame.triangles {
        if active_texture != Some(triangle.texture_id) {
            if active_texture.is_some() {
                glEnd();
            }
            if let Some(&handle) = material_textures
                .get(&triangle.texture_id)
                .filter(|_| textures)
            {
                glEnable(TEXTURE_2D);
                glBindTexture(TEXTURE_2D, handle);
                active_textured = true;
            } else {
                glDisable(TEXTURE_2D);
                active_textured = false;
            }
            glBegin(0x0004);
            active_texture = Some(triangle.texture_id);
        }
        if active_textured {
            textured += 1;
        }
        for vertex in &triangle.vertices {
            glColor3f(vertex.color[0], vertex.color[1], vertex.color[2]);
            glTexCoord2f(vertex.uv[0], vertex.uv[1]);
            glVertex3f(vertex.p[0], vertex.p[1], vertex.p[2]);
        }
    }
    if active_texture.is_some() {
        glEnd();
    }
    glPopMatrix();
    (frame.triangles.len(), textured)
}

unsafe fn ortho() {
    glDisable(DEPTH_TEST);
    glDisable(ALPHA_TEST);
    glDisable(FOG);
    glDisable(TEXTURE_2D);
    glMatrixMode(PROJECTION);
    glLoadIdentity();
    glOrtho(0.0, WIDTH as f64, HEIGHT as f64, 0.0, -1.0, 1.0);
    glMatrixMode(MODELVIEW);
    glLoadIdentity();
}

pub fn sky_atmosphere(hour: f32) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 4]) {
    if hour >= 22.0 || hour < 5.0 {
        // Deep Night
        (
            [0.03, 0.04, 0.09],
            [0.06, 0.08, 0.16],
            [0.05, 0.06, 0.12],
            [0.06, 0.08, 0.15, 1.0],
        )
    } else if hour >= 5.0 && hour < 8.0 {
        // Dawn
        let t = (hour - 5.0) / 3.0;
        (
            [0.15 + 0.25 * t, 0.12 + 0.35 * t, 0.25 + 0.45 * t],
            [0.35 + 0.45 * t, 0.25 + 0.40 * t, 0.30 + 0.50 * t],
            [0.15 + 0.10 * t, 0.15 + 0.25 * t, 0.20 + 0.28 * t],
            [0.30 + 0.25 * t, 0.25 + 0.40 * t, 0.35 + 0.45 * t, 1.0],
        )
    } else if hour >= 8.0 && hour < 17.5 {
        // Daytime tropical blue
        (
            [0.36, 0.60, 0.79],
            [0.51, 0.70, 0.89],
            [0.22, 0.41, 0.48],
            [0.55, 0.75, 0.90, 1.0],
        )
    } else if hour >= 17.5 && hour < 20.5 {
        // Iconic Vice City Sunset (neon pink & golden orange)
        let t = (hour - 17.5) / 3.0;
        (
            [0.36 + 0.52 * t, 0.60 - 0.38 * t, 0.79 - 0.30 * t],
            [0.51 + 0.45 * t, 0.70 - 0.20 * t, 0.89 - 0.50 * t],
            [0.22 + 0.20 * t, 0.41 - 0.25 * t, 0.48 - 0.15 * t],
            [0.70 * t + 0.55 * (1.0 - t), 0.35 * t + 0.75 * (1.0 - t), 0.55 * t + 0.90 * (1.0 - t), 1.0],
        )
    } else {
        // Dusk transitioning to night (20.5 - 22.0)
        let t = (hour - 20.5) / 1.5;
        (
            [0.81 - 0.78 * t, 0.25 - 0.21 * t, 0.49 - 0.40 * t],
            [0.96 - 0.90 * t, 0.50 - 0.42 * t, 0.39 - 0.23 * t],
            [0.42 - 0.37 * t, 0.16 - 0.10 * t, 0.33 - 0.21 * t],
            [0.70 - 0.64 * t, 0.35 - 0.27 * t, 0.55 - 0.40 * t, 1.0],
        )
    }
}

unsafe fn background(hour: f32) {
    let (top, mid, bot, _) = sky_atmosphere(hour);
    glDisable(BLEND);
    glBegin(QUADS);
    glColor3f(top[0], top[1], top[2]);
    glVertex2f(0.0, 0.0);
    glVertex2f(WIDTH as f32, 0.0);
    glColor3f(mid[0], mid[1], mid[2]);
    glVertex2f(WIDTH as f32, HEIGHT as f32 / 2.0);
    glVertex2f(0.0, HEIGHT as f32 / 2.0);
    glColor3f(bot[0], bot[1], bot[2]);
    glVertex2f(0.0, HEIGHT as f32 / 2.0);
    glVertex2f(WIDTH as f32, HEIGHT as f32 / 2.0);
    glVertex2f(WIDTH as f32, HEIGHT as f32);
    glVertex2f(0.0, HEIGHT as f32);
    glEnd();
    if hour >= 21.5 || hour < 5.0 {
        glBegin(0x0000); // GL_POINTS
        glColor4f(1.0, 1.0, 1.0, 0.85);
        for i in 0..48 {
            let sx = ((i * 137 + 43) % WIDTH) as f32;
            let sy = ((i * 79 + 19) % (HEIGHT / 2)) as f32;
            glVertex2f(sx, sy);
        }
        glEnd();
    }
}

unsafe fn draw_vehicle_effects(vehicles: &[VehicleFrame<'_>], global_time: f32, hour: f32) {
    glDisable(TEXTURE_2D);
    glEnable(BLEND);
    glBlendFunc(0x0302, 0x0303);
    let is_night = hour < 6.5 || hour > 19.5;

    for v in vehicles {
        let (s, c) = v.heading.sin_cos();

        // 1. Vehicle headlights on the pavement
        if is_night || v.headlights {
            glColor4f(1.0, 0.95, 0.7, 0.22);
            glBegin(TRIANGLES);
            // Left headlight beam
            let lx = v.position[0] - c * 0.65 + s * 1.6;
            let ly = v.position[1] + s * 0.65 + c * 1.6;
            let lz = v.position[2] + 0.15;
            let flx = lx + s * 20.0 - c * 3.0;
            let fly = ly + c * 20.0 + s * 3.0;
            let frx = lx + s * 20.0 + c * 1.2;
            let fry = ly + c * 20.0 - s * 1.2;
            glVertex3f(lx, ly, lz);
            glVertex3f(flx, fly, lz);
            glVertex3f(frx, fry, lz);

            // Right headlight beam
            let rx = v.position[0] + c * 0.65 + s * 1.6;
            let ry = v.position[1] - s * 0.65 + c * 1.6;
            let rz = v.position[2] + 0.15;
            let rlx = rx + s * 20.0 - c * 1.2;
            let rly = ry + c * 20.0 + s * 1.2;
            let rrx = rx + s * 20.0 + c * 3.0;
            let rry = ry + c * 20.0 - s * 3.0;
            glVertex3f(rx, ry, rz);
            glVertex3f(rlx, rly, rz);
            glVertex3f(rrx, rry, rz);
            glEnd();

            // Red rear tail lights
            glBegin(QUADS);
            glColor4f(1.0, 0.1, 0.05, 0.9);
            for side in [-0.65, 0.65] {
                let tx = v.position[0] + c * side - s * 1.8;
                let ty = v.position[1] - s * side - c * 1.8;
                let tz = v.position[2] + 0.5;
                glVertex3f(tx - 0.08, ty, tz - 0.06);
                glVertex3f(tx + 0.08, ty, tz - 0.06);
                glVertex3f(tx + 0.08, ty, tz + 0.06);
                glVertex3f(tx - 0.08, ty, tz + 0.06);
            }
            glEnd();
        }

        // 2. Police roof strobe lights
        if v.is_police {
            glBegin(QUADS);
            let z_roof = v.position[2] + 1.25;
            let bl_alpha = if v.strobe_timer < 0.5 { 0.95 } else { 0.1 };
            glColor4f(0.1, 0.4, 1.0, bl_alpha);
            let bx = v.position[0] - c * 0.35;
            let by = v.position[1] + s * 0.35;
            glVertex3f(bx - 0.15, by - 0.15, z_roof);
            glVertex3f(bx + 0.15, by - 0.15, z_roof);
            glVertex3f(bx + 0.15, by + 0.15, z_roof + 0.15);
            glVertex3f(bx - 0.15, by + 0.15, z_roof + 0.15);

            let rd_alpha = if v.strobe_timer >= 0.5 { 0.95 } else { 0.1 };
            glColor4f(1.0, 0.15, 0.1, rd_alpha);
            let rx = v.position[0] + c * 0.35;
            let ry = v.position[1] - s * 0.35;
            glVertex3f(rx - 0.15, ry - 0.15, z_roof);
            glVertex3f(rx + 0.15, ry - 0.15, z_roof);
            glVertex3f(rx + 0.15, ry + 0.15, z_roof + 0.15);
            glVertex3f(rx - 0.15, ry + 0.15, z_roof + 0.15);
            glEnd();
        }

        // 3. Smoke & Engine fire plumes
        if v.health < 400.0 || v.burning || v.exploded {
            let hx = v.position[0] + s * 1.35;
            let hy = v.position[1] + c * 1.35;
            let hz = v.position[2] + 0.8;

            if v.burning {
                glBegin(TRIANGLES);
                let flick = (global_time * 12.0).sin() * 0.15;
                glColor4f(1.0, 0.5 + flick, 0.05, 0.85);
                glVertex3f(hx - 0.35, hy, hz);
                glVertex3f(hx + 0.35, hy, hz);
                glVertex3f(hx, hy, hz + 0.9 + flick);

                glColor4f(1.0, 0.8, 0.2, 0.9);
                glVertex3f(hx, hy - 0.25, hz);
                glVertex3f(hx, hy + 0.25, hz);
                glVertex3f(hx, hy, hz + 0.6);
                glEnd();
            }

            glBegin(QUADS);
            let smoke_col = if v.exploded || v.burning { 0.15 } else { 0.65 };
            glColor4f(smoke_col, smoke_col, smoke_col, 0.55);
            for puff in 0..3 {
                let pz = hz + 0.4 + puff as f32 * 0.45;
                let sz = 0.25 + puff as f32 * 0.15;
                let drift = (global_time * 2.0 + puff as f32).sin() * 0.1;
                glVertex3f(hx - sz + drift, hy, pz - sz);
                glVertex3f(hx + sz + drift, hy, pz - sz);
                glVertex3f(hx + sz + drift, hy, pz + sz);
                glVertex3f(hx - sz + drift, hy, pz + sz);
            }
            glEnd();
        }
    }
    glDisable(BLEND);
}

unsafe fn quad(x: f32, y: f32, width: f32, height: f32) {
    glVertex2f(x, y);
    glVertex2f(x + width, y);
    glVertex2f(x + width, y + height);
    glVertex2f(x, y + height);
}

unsafe fn text(x: f32, y: f32, line: &str, color: [f32; 3]) {
    glColor4f(color[0], color[1], color[2], 1.0);
    glBegin(QUADS);
    for (letter, ch) in line.chars().enumerate() {
        let glyph = crate::hud::glyph(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    quad(
                        x + letter as f32 * 12.0 + column as f32 * 2.0,
                        y + row as f32 * 2.0,
                        2.0,
                        2.0,
                    );
                }
            }
        }
    }
    glEnd();
}

unsafe fn draw_mission_corona(marker: Option<[f32; 3]>, pulse: f32) {
    let Some(pos) = marker else { return };
    glDisable(TEXTURE_2D);
    glEnable(BLEND);
    glBlendFunc(0x0302, 0x0303);

    let base_z = pos[2];
    let height = 2.4;
    let radius = 2.8;
    let alpha = 0.40 + 0.18 * pulse.sin();
    let segments = 16;

    glBegin(QUADS);
    glColor4f(1.0, 0.15, 0.75, alpha);
    for i in 0..segments {
        let a1 = i as f32 * std::f32::consts::TAU / segments as f32;
        let a2 = (i + 1) as f32 * std::f32::consts::TAU / segments as f32;
        let x1 = pos[0] + a1.cos() * radius;
        let y1 = pos[1] + a1.sin() * radius;
        let x2 = pos[0] + a2.cos() * radius;
        let y2 = pos[1] + a2.sin() * radius;

        glVertex3f(x1, y1, base_z);
        glVertex3f(x2, y2, base_z);
        glVertex3f(x2, y2, base_z + height);
        glVertex3f(x1, y1, base_z + height);
    }
    glEnd();

    glBegin(LINES);
    glColor4f(1.0, 0.85, 0.95, alpha + 0.2);
    for i in 0..segments {
        let a1 = i as f32 * std::f32::consts::TAU / segments as f32;
        let a2 = (i + 1) as f32 * std::f32::consts::TAU / segments as f32;
        let x1 = pos[0] + a1.cos() * radius;
        let y1 = pos[1] + a1.sin() * radius;
        let x2 = pos[0] + a2.cos() * radius;
        let y2 = pos[1] + a2.sin() * radius;
        glVertex3f(x1, y1, base_z + height);
        glVertex3f(x2, y2, base_z + height);
    }
    glEnd();
    glDisable(BLEND);
}

unsafe fn draw_mission_hud(banner: Option<&str>, objective: Option<&str>) {
    glEnable(BLEND);
    glBlendFunc(0x0302, 0x0303);
    if let Some(title) = banner {
        glColor4f(0.04, 0.04, 0.08, 0.75);
        glBegin(QUADS);
        let len = title.len() as f32 * 12.0 + 30.0;
        let x = (WIDTH as f32 - len) * 0.5;
        quad(x, 115.0, len, 32.0);
        glEnd();
        text(x + 15.0, 122.0, title, [1.0, 0.88, 0.15]);
    }
    if let Some(sub) = objective {
        glColor4f(0.02, 0.04, 0.06, 0.70);
        glBegin(QUADS);
        let len = sub.len() as f32 * 12.0 + 30.0;
        let x = (WIDTH as f32 - len) * 0.5;
        quad(x, HEIGHT as f32 - 110.0, len, 28.0);
        glEnd();
        text(x + 15.0, HEIGHT as f32 - 104.0, sub, [0.85, 0.95, 1.0]);
    }
    glDisable(BLEND);
}

unsafe fn draw_guide(zone: &str, walking: bool, driving: bool, show_help: bool) {
    if !show_help {
        return;
    }
    glEnable(BLEND);
    glBlendFunc(0x0302, 0x0303); // SRC_ALPHA, ONE_MINUS_SRC_ALPHA
    glColor4f(0.02, 0.04, 0.06, 0.75);
    glBegin(QUADS);
    quad(10.0, 10.0, 540.0, 30.0);
    if show_help {
        quad(10.0, HEIGHT as f32 - 67.0, WIDTH as f32 - 20.0, 57.0);
    }
    glEnd();
    let mode = if driving {
        "CONDUITE"
    } else if walking {
        "MARCHE EXPERIMENTALE"
    } else {
        "CAMERA LIBRE"
    };
    text(20.0, 19.0, &format!("VCS RUST | {zone} | {mode}"), [1.0; 3]);
    if show_help {
        text(
            20.0,
            HEIGHT as f32 - 57.0,
            if driving {
                "Z/W: ACCELERER | S: FREIN / RECUL | ESPACE: FREIN A MAIN"
            } else {
                "ZQSD / WASD: DEPLACER | FLECHES: VUE | MAJ: ACCELERER"
            },
            [1.0; 3],
        );
        text(
            20.0,
            HEIGHT as f32 - 42.0,
            if driving {
                "Q/A-D: CONDUIRE | H: GIROPHARE | 1-4: LIEU | T: TEXTURES"
            } else if walking {
                "ESPACE: SAUT | TAB: ARME | CLIC G/F: TIR | CLIC D/V: VISER"
            } else {
                "ESPACE / CTRL: HAUTEUR | 1-4: LIEU | T: TEXTURES"
            },
            [1.0; 3],
        );
        text(
            20.0,
            HEIGHT as f32 - 27.0,
            if driving {
                "ARRET + E: SORTIR | R: REINITIALISER | ECHAP: QUITTER"
            } else {
                "F5: SAUVER | F9: CHARGER | G: MARCHE | E: VEHICULE | F1: AIDE"
            },
            [0.74, 0.92, 1.0],
        );
    }
    glDisable(BLEND);
}


unsafe fn draw_traffic_lights(lights: &[crate::traffic::TrafficLight], time: f32) {
    glDisable(TEXTURE_2D);
    glDisable(BLEND);
    for light in lights {
        let state = light.state_at(time);
        let x = light.position[0];
        let y = light.position[1];
        let z = light.position[2];

        // Vertical post
        glColor3f(0.25, 0.25, 0.28);
        glBegin(LINES);
        glVertex3f(x, y, z);
        glVertex3f(x, y, z + 4.6);
        glEnd();

        // Housing box
        glColor3f(0.12, 0.12, 0.15);
        glBegin(QUADS);
        glVertex3f(x - 0.25, y - 0.25, z + 3.5);
        glVertex3f(x + 0.25, y - 0.25, z + 3.5);
        glVertex3f(x + 0.25, y - 0.25, z + 4.7);
        glVertex3f(x - 0.25, y - 0.25, z + 4.7);
        glEnd();

        // Red lens
        if state == crate::traffic::TrafficLightState::Red {
            glColor3f(1.0, 0.08, 0.08);
        } else {
            glColor3f(0.25, 0.04, 0.04);
        }
        glBegin(QUADS);
        glVertex3f(x - 0.16, y - 0.26, z + 4.35);
        glVertex3f(x + 0.16, y - 0.26, z + 4.35);
        glVertex3f(x + 0.16, y - 0.26, z + 4.65);
        glVertex3f(x - 0.16, y - 0.26, z + 4.65);
        glEnd();

        // Amber lens
        if state == crate::traffic::TrafficLightState::Amber {
            glColor3f(1.0, 0.75, 0.1);
        } else {
            glColor3f(0.25, 0.18, 0.04);
        }
        glBegin(QUADS);
        glVertex3f(x - 0.16, y - 0.26, z + 3.95);
        glVertex3f(x + 0.16, y - 0.26, z + 3.95);
        glVertex3f(x + 0.16, y - 0.26, z + 4.25);
        glVertex3f(x - 0.16, y - 0.26, z + 4.25);
        glEnd();

        // Green lens
        if state == crate::traffic::TrafficLightState::Green {
            glColor3f(0.1, 1.0, 0.3);
        } else {
            glColor3f(0.04, 0.22, 0.06);
        }
        glBegin(QUADS);
        glVertex3f(x - 0.16, y - 0.26, z + 3.55);
        glVertex3f(x + 0.16, y - 0.26, z + 3.55);
        glVertex3f(x + 0.16, y - 0.26, z + 3.85);
        glVertex3f(x - 0.16, y - 0.26, z + 3.85);
        glEnd();
    }
}

unsafe fn draw_weapons_3d(arsenal: &crate::weapons::WeaponArsenal) {
    glDisable(TEXTURE_2D);
    glEnable(BLEND);
    glBlendFunc(0x0302, 0x0303);

    for tracer in &arsenal.tracers {
        let alpha = (tracer.lifetime / tracer.max_lifetime).clamp(0.0, 1.0);
        glColor4f(tracer.color[0], tracer.color[1], tracer.color[2], tracer.color[3] * alpha);
        glBegin(LINES);
        glVertex3f(tracer.start[0], tracer.start[1], tracer.start[2]);
        glVertex3f(tracer.end[0], tracer.end[1], tracer.end[2]);
        glEnd();
    }

    for proj in &arsenal.projectiles {
        let x = proj.position[0];
        let y = proj.position[1];
        let z = proj.position[2];
        let r = proj.radius;
        match proj.kind {
            crate::weapons::ProjectileKind::Rocket => {
                glColor3f(0.9, 0.2, 0.1);
                glBegin(TRIANGLES);
                glVertex3f(x, y + r * 2.0, z);
                glVertex3f(x - r, y, z - r);
                glVertex3f(x + r, y, z - r);
                glVertex3f(x, y + r * 2.0, z);
                glVertex3f(x + r, y, z - r);
                glVertex3f(x, y, z + r);
                glEnd();
            }
            crate::weapons::ProjectileKind::Grenade => {
                glColor3f(0.25, 0.4, 0.2);
                glBegin(QUADS);
                glVertex3f(x - r, y - r, z - r);
                glVertex3f(x + r, y - r, z - r);
                glVertex3f(x + r, y + r, z - r);
                glVertex3f(x - r, y + r, z - r);
                glVertex3f(x - r, y - r, z + r);
                glVertex3f(x + r, y - r, z + r);
                glVertex3f(x + r, y + r, z + r);
                glVertex3f(x - r, y + r, z + r);
                glEnd();
            }
            crate::weapons::ProjectileKind::Flame => {
                glColor4f(1.0, 0.55, 0.1, 0.85);
                glBegin(QUADS);
                glVertex3f(x - r, y, z - r);
                glVertex3f(x + r, y, z - r);
                glVertex3f(x + r, y, z + r);
                glVertex3f(x - r, y, z + r);
                glEnd();
            }
        }
    }

    for p in &arsenal.particles {
        let x = p.position[0];
        let y = p.position[1];
        let z = p.position[2];
        let s = p.size;
        glColor4f(p.color[0], p.color[1], p.color[2], p.color[3]);
        glBegin(QUADS);
        glVertex3f(x - s, y, z - s);
        glVertex3f(x + s, y, z - s);
        glVertex3f(x + s, y, z + s);
        glVertex3f(x - s, y, z + s);
        glEnd();
    }
    glDisable(BLEND);
}

unsafe fn draw_crosshair_hud(kind: crate::weapons::CrosshairKind) {
    let cx = WIDTH as f32 / 2.0;
    let cy = HEIGHT as f32 / 2.0;
    glDisable(TEXTURE_2D);

    match kind {
        crate::weapons::CrosshairKind::None => {}
        crate::weapons::CrosshairKind::DotCross => {
            glEnable(BLEND);
            glBlendFunc(0x0302, 0x0303);
            glColor4f(0.0, 0.95, 1.0, 0.85);
            glBegin(QUADS);
            quad(cx - 1.5, cy - 1.5, 3.0, 3.0);
            quad(cx - 14.0, cy - 1.0, 7.0, 2.0);
            quad(cx + 7.0, cy - 1.0, 7.0, 2.0);
            quad(cx - 1.0, cy - 14.0, 2.0, 7.0);
            quad(cx - 1.0, cy + 7.0, 2.0, 7.0);
            glEnd();
            glDisable(BLEND);
        }
        crate::weapons::CrosshairKind::CircleSpread => {
            glEnable(BLEND);
            glBlendFunc(0x0302, 0x0303);
            glColor4f(1.0, 0.75, 0.2, 0.85);
            glBegin(QUADS);
            quad(cx - 16.0, cy - 16.0, 6.0, 2.0);
            quad(cx - 16.0, cy - 16.0, 2.0, 6.0);
            quad(cx + 10.0, cy - 16.0, 6.0, 2.0);
            quad(cx + 14.0, cy - 16.0, 2.0, 6.0);
            quad(cx - 16.0, cy + 14.0, 6.0, 2.0);
            quad(cx - 16.0, cy + 10.0, 2.0, 6.0);
            quad(cx + 10.0, cy + 14.0, 6.0, 2.0);
            quad(cx + 14.0, cy + 10.0, 2.0, 6.0);
            glEnd();
            glDisable(BLEND);
        }
        crate::weapons::CrosshairKind::SniperScope => {
            glEnable(BLEND);
            glBlendFunc(0x0302, 0x0303);
            glColor4f(0.04, 0.02, 0.08, 0.92);
            glBegin(QUADS);
            quad(0.0, 0.0, cx - 180.0, HEIGHT as f32);
            quad(cx + 180.0, 0.0, cx - 180.0, HEIGHT as f32);
            quad(cx - 180.0, 0.0, 360.0, cy - 180.0);
            quad(cx - 180.0, cy + 180.0, 360.0, cy - 180.0);
            glEnd();

            glColor4f(0.9, 0.1, 0.2, 0.9);
            glBegin(QUADS);
            quad(cx - 180.0, cy - 0.75, 360.0, 1.5);
            quad(cx - 0.75, cy - 180.0, 1.5, 360.0);
            quad(cx - 2.0, cy - 2.0, 4.0, 4.0);
            for &mark in &[30.0, 60.0, 90.0, 120.0] {
                quad(cx - 6.0, cy + mark, 12.0, 1.5);
                quad(cx + mark, cy - 4.0, 1.5, 8.0);
                quad(cx - mark, cy - 4.0, 1.5, 8.0);
            }
            glEnd();
            glDisable(BLEND);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sky_atmosphere_gradient_matches_vice_city_palette() {
        let (top_noon, mid_noon, _, _) = sky_atmosphere(12.0);
        assert_eq!(top_noon, [0.36, 0.60, 0.79]);
        assert_eq!(mid_noon, [0.51, 0.70, 0.89]);

        let (top_sunset, _, _, _) = sky_atmosphere(19.0);
        assert!(top_sunset[0] > 0.55, "Sunset top should have strong magenta/red component");

        let (top_night, _, _, _) = sky_atmosphere(2.0);
        assert!(top_night[0] < 0.1, "Night top should be dark");
    }

    #[test]
    fn vehicle_frame_holds_damage_and_police_attributes() {
        let frame = VehicleFrame {
            triangles: &[],
            position: [10.0, 20.0, 3.0],
            heading: 1.57,
            health: 800.0,
            burning: false,
            exploded: false,
            is_police: true,
            strobe_timer: 0.25,
            headlights: true,
        };
        assert_eq!(frame.health, 800.0);
        assert!(frame.is_police);
    }

    #[test]
    fn windows_pixel_descriptor_has_native_size() {
        assert_eq!(std::mem::size_of::<PixelFormat>(), 40);
    }
    #[test]
    fn view_matrix_matches_software_camera_coordinates() {
        let camera = Camera {
            pos: [12.0, -9.0, 4.0],
            yaw: 0.7,
            pitch: -0.4,
        };
        let point = [30.0, 5.0, 17.0];
        let matrix = view_matrix(&camera);
        let actual: [f32; 3] = std::array::from_fn(|r| {
            matrix[r] * point[0]
                + matrix[4 + r] * point[1]
                + matrix[8 + r] * point[2]
                + matrix[12 + r]
        });
        let d = [
            point[0] - camera.pos[0],
            point[1] - camera.pos[1],
            point[2] - camera.pos[2],
        ];
        let (s, c) = camera.yaw.sin_cos();
        let (sp, cp) = camera.pitch.sin_cos();
        let z = s * d[0] + c * d[1];
        let expected = [
            c * d[0] - s * d[1],
            cp * d[2] - sp * z,
            -(sp * d[2] + cp * z),
        ];
        for i in 0..3 {
            assert!((actual[i] - expected[i]).abs() < 0.0001);
        }
    }

    #[test]
    fn opaque_surfaces_precede_back_to_front_transparent_instances() {
        let camera = Camera {
            pos: [10.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
        };
        let instance = |id, pass, x| Instance {
            id,
            resource: id,
            pass,
            matrix: [0.0; 16],
            sphere: [x, 0.0, 0.0, 1.0],
        };
        let mut instances = [
            instance(1, 7, 20.0),
            instance(2, 5, -100.0),
            instance(3, 4, 1000.0),
        ];
        instances.sort_by(|a, b| instance_order(a, b, &camera));
        assert_eq!(instances.map(|i| i.id), [3, 2, 1]);
    }

    #[test]
    fn transparent_sort_uses_translation_when_bound_center_is_invalid() {
        let camera = Camera {
            pos: [10.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
        };
        let mut instance = Instance {
            id: 0,
            resource: 0,
            pass: 7,
            matrix: [0.0; 16],
            sphere: [f32::NAN; 4],
        };
        instance.matrix[12..15].copy_from_slice(&[13.0, 4.0, 0.0]);
        assert_eq!(instance_distance_squared(&instance, &camera), 25.0);
    }

    #[test]
    fn character_heading_rotates_local_forward_to_world_heading() {
        for heading in [0.0, std::f32::consts::FRAC_PI_2, -0.7] {
            let matrix = character_matrix([10.0, 20.0, 30.0], heading);
            // Transform local +Y relative to the feet anchor.
            assert!((matrix[4] - heading.sin()).abs() < 0.00001);
            assert!((matrix[5] - heading.cos()).abs() < 0.00001);
            assert_eq!(matrix[6], 0.0);
            assert_eq!(&matrix[12..15], &[10.0, 20.0, 30.0]);
            assert_eq!(
                [matrix[3], matrix[7], matrix[11], matrix[15]],
                [0.0, 0.0, 0.0, 1.0]
            );
        }
    }
}
