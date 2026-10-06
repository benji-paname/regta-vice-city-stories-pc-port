#![cfg_attr(windows, windows_subsystem = "windows")]

mod character;
mod collision;
mod gpu;
mod hud;
mod iso;
mod leeds;
mod player;
mod render;
mod scene;
mod texture;
mod vehicle;
use iso::{Disc, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const DEFAULT_ISO: &str = r"F:\JEUX EMULATION\PS2 GAMES\Grand Theft Auto - Vice City Stories.iso";
fn main() {
    if let Err(error) = run() {
        eprintln!("VCS Rust: {error}");
        if let Ok(exe) = std::env::current_exe() {
            if let Some(folder) = exe.parent() {
                let _ = std::fs::write(folder.join("erreur.txt"), format!("{error}\n"));
            }
        }
        #[cfg(windows)]
        {
            if !std::env::args().any(|arg| {
                arg == "--verify"
                    || arg == "--preview"
                    || arg == "--smoke"
                    || arg == "--capture"
                    || arg == "--headless"
            }) {
                show_error(&error.to_string());
            }
        }
        std::process::exit(1);
    }
}
#[cfg(windows)]
fn show_error(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            title: *const u16,
            kind: u32,
        ) -> i32;
    }
    let text:Vec<u16>=format!("Impossible d'ouvrir le prototype VCS Rust.\n\n{message}\n\nLe detail est conserve dans erreur.txt.").encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "VCS Rust".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let zone = args
        .iter()
        .position(|s| s == "--zone")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
        .unwrap_or("BEACH");
    let zone_index = match zone {
        "BEACH" => 0,
        "MAINLA" => 1,
        "MALL" => 2,
        _ => return Err("--zone must be BEACH, MAINLA or MALL".into()),
    };
    let iso = args
        .iter()
        .position(|s| s == "--iso")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ISO));
    let root = std::env::current_exe()?
        .parent()
        .ok_or("Executable directory missing")?
        .to_path_buf();
    let out = if let Some(i) = args.iter().position(|s| s == "--output") {
        PathBuf::from(args.get(i + 1).ok_or("--output needs a folder")?)
    } else {
        root.join("data")
    };
    std::fs::create_dir_all(&out)?;
    let mut disc = Disc::open(&iso)?;
    let cnf = String::from_utf8(disc.read("SYSTEM.CNF")?)?;
    let executable = cnf
        .lines()
        .find(|l| l.trim_start().starts_with("BOOT2"))
        .and_then(|l| l.split('\\').next_back())
        .and_then(|s| s.split(';').next())
        .ok_or("Missing BOOT2 entry")?
        .trim()
        .to_string();
    if executable != "SLES_546.22" {
        return Err(format!(
            "Unsupported edition {executable}; this prototype is verified for SLES_546.22 PAL"
        )
        .into());
    }
    let elf = disc.read(&executable)?;
    let hash = format!("{:x}", Sha256::digest(&elf));
    if hash != "1039bd48fbe57f3961c72507bfce526ef524dbe9b374d844a00b9a2db435439d" {
        return Err("The ELF differs from the verified PS2 edition".into());
    }
    println!(
        "VCS Rust {} - PS2 PAL SLES-54622 - continuous world and original animated player",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "ISO: {} ({} bytes, {} files/directories)",
        iso.display(),
        disc.size,
        disc.entries.len()
    );
    let mut levels = Vec::new();
    for name in ["BEACH", "MAINLA", "MALL"] {
        println!("Loading {name}...");
        let level = leeds::load_level(&mut disc, name)?;
        println!("{}", serde_json::to_string_pretty(&level.stats)?);
        std::fs::write(
            out.join(format!("{name}-instances.json")),
            serde_json::to_vec_pretty(&level.instances)?,
        )?;
        levels.push(level);
    }
    let game = leeds::inflate(&disc.read("PS2/GAME.DTZ")?)?;
    if iso::bytes(&game, 0, 4)? != b"GATG" || iso::u32le(&game, 8)? as usize != game.len() {
        return Err("Invalid GAME.DTZ resource image".into());
    }
    println!("Loading original static collision models...");
    let mut collisions = collision::load_world(&mut disc, &game)?;
    println!("{}", serde_json::to_string_pretty(&collisions.stats)?);
    let mut ground_checks = Vec::new();
    for index in 0..3 {
        scene::select_collisions(&mut collisions, index);
        let camera = render::Camera::walking_spawn(index);
        let contact = collisions
            .raycast(camera.pos, [0.0, 0.0, -1.0], 800.0)
            .ok_or("No collision contact at spawn")?;
        let mut walker = player::Walker::from_camera(&collisions, &camera).ok_or_else(|| {
            format!(
                "No clear native walking spawn in {}",
                levels[index].stats.name
            )
        })?;
        let initial = walker.feet;
        for _ in 0..120 {
            walker.step(&collisions, [0.0; 2], 4.5, false, 1.0 / 120.0);
        }
        if !walker.grounded || (walker.feet[2] - initial[2]).abs() > 0.005 {
            return Err(format!("Unstable native ground for {}", levels[index].stats.name).into());
        }
        ground_checks.push(serde_json::json!({"zone":levels[index].stats.name,"feet":walker.feet,"native_model_id":contact.model_id,"surface":contact.surface,"stationary_physics_steps":120,"grounded":walker.grounded}));
    }
    scene::select_collisions(&mut collisions, zone_index);
    println!("Loading original player and native animation clips...");
    let character = character::Character::load(&mut disc, &game)?;
    println!("{}", serde_json::to_string_pretty(&character.stats)?);
    let mut pose_checks = Vec::new();
    for clip in &character.stats.clips {
        let first = character.triangles(&clip.name, 0.0);
        let later = character.triangles(&clip.name, clip.duration_seconds * 0.37);
        let looped = character.triangles(&clip.name, clip.duration_seconds);
        let mut variation = 0.0f32;
        let mut seam = 0.0f32;
        for ((a, b), c) in first.iter().zip(&later).zip(&looped) {
            for i in 0..3 {
                for j in 0..3 {
                    let x = a.vertices[i].p[j];
                    let y = b.vertices[i].p[j];
                    let z = c.vertices[i].p[j];
                    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                        return Err("Non-finite native character pose".into());
                    }
                    variation = variation.max((x - y).abs());
                    seam = seam.max((x - z).abs());
                }
            }
        }
        if first.len() != character.stats.drawable_triangles
            || variation < 0.00001
            || seam > 0.00001
        {
            return Err(format!("Native animation validation failed: {}", clip.name).into());
        }
        pose_checks.push(serde_json::json!({"clip":clip.name,"max_position_variation_m":variation,"max_loop_error_m":seam,"finite":true}));
    }
    println!("Loading original first vehicle (Banshee) and shared wheels...");
    let vehicle = vehicle::VehicleModel::load_banshee(&mut disc, &game)?;
    println!("{}", serde_json::to_string_pretty(&vehicle.stats)?);
    std::fs::write(
        out.join("vehicle-validation.json"),
        serde_json::to_vec_pretty(&vehicle.stats)?,
    )?;
    let camera_override = if let Some(i) = args.iter().position(|s| s == "--camera") {
        let values: Vec<f32> = args
            .get(i + 1)
            .ok_or("--camera needs x,y,z,yaw,pitch")?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        if values.len() != 5 || values.iter().any(|v| !v.is_finite()) {
            return Err("Invalid camera coordinates".into());
        }
        Some(render::Camera {
            pos: [values[0], values[1], values[2]],
            yaw: values[3],
            pitch: values[4],
        })
    } else {
        None
    };
    let animation = args
        .iter()
        .position(|s| s == "--animation")
        .map(|i| {
            args.get(i + 1)
                .cloned()
                .ok_or("--animation needs idle,walk,run or a native clip name")
        })
        .transpose()?;
    if let Some(name) = &animation {
        if !matches!(name.as_str(), "idle" | "walk" | "run" | "bind")
            && !character
                .stats
                .clips
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(name))
        {
            return Err("Unsupported character animation".into());
        }
    }
    let animation_time = args
        .iter()
        .position(|s| s == "--animation-time")
        .map(|i| {
            args.get(i + 1)
                .ok_or("--animation-time needs seconds")?
                .parse::<f32>()
                .map_err(|_| "Invalid animation time")
        })
        .transpose()?;
    if animation_time.is_some_and(|t| !t.is_finite()) {
        return Err("Animation time must be finite".into());
    }
    let simulate_seconds = args
        .iter()
        .position(|s| s == "--simulate")
        .map(|i| {
            args.get(i + 1)
                .ok_or("--simulate needs seconds")?
                .parse::<f32>()
                .map_err(|_| "Invalid simulation time")
        })
        .transpose()?
        .unwrap_or(0.0);
    if !simulate_seconds.is_finite() || !(0.0..=30.0).contains(&simulate_seconds) {
        return Err("Simulation time must be between zero and 30 seconds".into());
    }
    let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"status":"continuous-map-and-animated-player-prototype","iso":iso,"executable":executable,"elf_sha256":hash,
        "game_dat_bytes":game.len(),"iso_entries":disc.entries,"levels":levels.iter().map(|l|&l.stats).collect::<Vec<_>>(),
        "collision":collisions.stats,"ground_checks":ground_checks,"character":character.stats,"pose_checks":pose_checks,"vehicle":vehicle.stats,
        "limitations":["No original game logic, missions, AI or original pedestrian physics","Vehicle rendering integrated for Banshee; driving physics, player entry/exit and traffic remain in development","Original character rotations; root translation locked, no gait IK or transition blending","Initial static colliders; scripted swaps and dynamic object activation are not implemented","Visual visibility uses noon and default script state; LOD, underwater passes and additional interiors remain incomplete"]});
    std::fs::write(
        out.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Verification written to {}",
        out.join("verification.json").display()
    );
    if let Some(i) = args.iter().position(|s| s == "--preview") {
        let path = PathBuf::from(args.get(i + 1).ok_or("--preview needs a BMP path")?);
        let mut frame = render::Frame::new();
        let started = std::time::Instant::now();
        let walking = args.iter().any(|s| s == "--walk");
        scene::select_collisions(&mut collisions, zone_index);
        let mut camera = camera_override.unwrap_or_else(|| {
            if walking {
                render::Camera::walking_spawn(zone_index)
            } else {
                render::Camera::for_zone(zone_index)
            }
        });
        let walker = if walking {
            Some(player::Walker::from_camera(&collisions, &camera).ok_or("No ground for preview")?)
        } else {
            None
        };
        let third_person = !args.iter().any(|s| s == "--first-person");
        let posed = character.triangles(
            animation.as_deref().unwrap_or("idle"),
            animation_time.unwrap_or(0.0),
        );
        let figure = if third_person {
            walker.as_ref().map(|w| gpu::CharacterFrame {
                triangles: &posed,
                position: w.feet,
                heading: camera.yaw,
            })
        } else {
            None
        };
        if let Some(walker) = &walker {
            camera = if third_person {
                render::follow_camera(&camera, walker.feet, &collisions)
            } else {
                render::Camera {
                    pos: walker.eye(),
                    ..camera
                }
            };
        }
        let vehicle_pos = match zone_index {
            1 => [-1005.0, -495.0, 11.0],
            2 => [385.0, 1125.0, -212.0],
            _ => [504.0, -496.0, 10.5],
        };
        let vehicle_frame = gpu::VehicleFrame {
            triangles: &vehicle.triangles,
            position: vehicle_pos,
            heading: -1.1,
        };
        frame.draw_scene_all(
            &levels,
            zone_index,
            &camera,
            true,
            figure.as_ref().map(|f| (f, &character.textures)),
            Some((&vehicle_frame, &vehicle.textures)),
        );
        hud::guide(&mut frame.pixels, scene::label(zone_index), walking, true);
        frame.write_bmp(&path)?;
        println!(
            "Native render: {} triangles, {} covered pixels",
            frame.triangles,
            frame.covered_pixels()
        );
        if frame.covered_pixels() < 1000 {
            return Err(
                "Preview contains too little geometry; camera or decoding needs investigation"
                    .into(),
            );
        }
        println!("Render time including BMP output: {:?}", started.elapsed());
    }
    if !args.iter().any(|s| s == "--verify" || s == "--preview") {
        let capture = args
            .iter()
            .position(|s| s == "--capture")
            .map(|i| {
                args.get(i + 1)
                    .map(PathBuf::from)
                    .ok_or("--capture needs a BMP path")
            })
            .transpose()?;
        let headless = args.iter().any(|s| s == "--headless");
        if simulate_seconds > 0.0 && !headless {
            return Err("Automatic simulation requires --headless".into());
        }
        let smoke = args.iter().any(|s| s == "--smoke") || capture.is_some() || headless;
        let viewer_report = render::viewer(
            &levels,
            &mut collisions,
            &character,
            &vehicle,
            render::ViewerOptions {
                smoke,
                initial: zone_index,
                start_walking: args.iter().any(|s| s == "--walk")
                    || (!smoke && !args.iter().any(|s| s == "--fly")),
                software: args.iter().any(|s| s == "--software"),
                capture,
                headless,
                initial_camera: camera_override,
                third_person: !args.iter().any(|s| s == "--first-person"),
                animation,
                animation_time,
                simulate_seconds,
            },
        )?;
        if smoke {
            std::fs::write(
                out.join("viewer-verification.json"),
                serde_json::to_vec_pretty(&viewer_report)?,
            )?;
        }
    }
    Ok(())
}
