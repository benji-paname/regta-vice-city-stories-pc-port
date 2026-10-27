#![cfg_attr(windows, windows_subsystem = "windows")]

mod access;
mod audio;
mod audit;
mod character;
mod collision;
mod combat;
mod driving;
mod gpu;
mod hud;
mod iso;
mod leeds;
mod locomotion;
mod modern;
mod objects;
mod paths;
mod player;
mod population;
mod props;
mod render;
mod scene;
mod script;
mod settings;
mod texture;
mod traffic;
mod vehicle;
mod water;
mod police;
mod save;
mod weapons;
mod weather;
mod transmission;
mod tires;
mod projectiles;
mod panic;
mod crosshair;
mod watercraft;
use iso::{Disc, Result};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const DEFAULT_ISO: &str = r"C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\work\Grand Theft Auto - Vice City Stories.iso";
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
    let game = leeds::inflate(&disc.read("PS2/GAME.DTZ")?)?;
    if iso::bytes(&game, 0, 4)? != b"GATG" || iso::u32le(&game, 8)? as usize != game.len() {
        return Err("Invalid GAME.DTZ resource image".into());
    }
    let selected_vehicle_id: u16 = if let Some(i) = args.iter().position(|arg| arg == "--vehicle") {
        let spec = args
            .get(i + 1)
            .ok_or("--vehicle needs an ID (170..280) or vehicle name")?;
        if let Ok(id) = spec.parse::<u16>() {
            id
        } else if let Some(entry) = vehicle::VehicleCatalog::find(spec) {
            entry.id
        } else {
            return Err(format!("Vehicle '{spec}' not found in catalog").into());
        }
    } else {
        175 // Default Banshee
    };
    if let Some(i) = args.iter().position(|arg| arg == "--vehicle-preview") {
        let path = PathBuf::from(
            args.get(i + 1)
                .ok_or("--vehicle-preview needs a BMP path")?,
        );
        let vehicle = vehicle::VehicleModel::load(&mut disc, &game, selected_vehicle_id)?;
        let camera = render::Camera {
            pos: [2.66, -5.38, 2.8],
            yaw: -0.459,
            pitch: -0.25,
        };
        let vehicle_frame = gpu::VehicleFrame {
            triangles: &vehicle.triangles,
            position: [0.0; 3],
            heading: -1.1,
            health: 1000.0,
            burning: false,
            exploded: false,
            is_police: false,
            strobe_timer: 0.0,
            headlights: true,
        };
        let ground_vertex = |p| leeds::Vertex {
            p,
            color: [0.29, 0.32, 0.28],
            uv: [0.0; 2],
        };
        let ground = [
            leeds::Triangle {
                vertices: [
                    ground_vertex([-30.0, -30.0, -0.02]),
                    ground_vertex([30.0, -30.0, -0.02]),
                    ground_vertex([30.0, 30.0, -0.02]),
                ],
                texture_id: 0,
            },
            leeds::Triangle {
                vertices: [
                    ground_vertex([-30.0, -30.0, -0.02]),
                    ground_vertex([30.0, 30.0, -0.02]),
                    ground_vertex([-30.0, 30.0, -0.02]),
                ],
                texture_id: 0,
            },
        ];
        let ground_frame = gpu::CharacterFrame {
            triangles: &ground,
            position: [0.0; 3],
            heading: 0.0,
        };
        let mut frame = render::Frame::new();
        frame.draw_scene_all(
            &[],
            0,
            &camera,
            true,
            Some((&ground_frame, &vehicle.textures)),
            Some((&vehicle_frame, &vehicle.textures)),
        );
        frame.write_bmp(&path)?;
        println!("Vehicle-only preview: {} triangles", frame.triangles);
        return Ok(());
    }
    let mut levels = Vec::new();
    let level_names: &[&str] = if args.iter().any(|arg| arg == "--preview") && zone_index != 2 {
        &["BEACH", "MAINLA"]
    } else {
        &["BEACH", "MAINLA", "MALL"]
    };
    for name in level_names {
        println!("Loading {name}...");
        let level = leeds::load_level(&mut disc, name)?;
        println!("{}", serde_json::to_string_pretty(&level.stats)?);
        std::fs::write(
            out.join(format!("{name}-instances.json")),
            serde_json::to_vec_pretty(&level.instances)?,
        )?;
        levels.push(level);
    }
    let furniture = props::attach(&mut disc, &game, &mut levels)?;
    println!("Native street furniture restored: {}", furniture.restored);
    let water = water::attach(&game, &mut levels[0])?;
    let water_grid = water::WaterGrid::load(&game)?;
    println!(
        "Native water: {} cells, {} patches",
        water.cells, water.patches
    );
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
            || (variation < 0.00001 && clip.name != "CAR_Lsit")
            || seam > 0.00001
        {
            return Err(format!("Native animation validation failed: {}", clip.name).into());
        }
        pose_checks.push(serde_json::json!({"clip":clip.name,"max_position_variation_m":variation,"max_loop_error_m":seam,"finite":true}));
    }
    let vehicle_entry = vehicle::VehicleCatalog::get(selected_vehicle_id).unwrap_or(&vehicle::VEHICLE_CATALOG[5]);
    println!("Loading original vehicle {} (ID {}) [type: {:?}]...", vehicle_entry.display_name, vehicle_entry.id, vehicle_entry.vehicle_type);
    if vehicle_entry.is_watercraft() {
        println!("Aquatic mechanics active: hydrodynamic buoyancy, planing, spray and wake simulation enabled.");
    }
    let vehicle = vehicle::VehicleModel::load(&mut disc, &game, selected_vehicle_id)?;
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
    let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"status":"continuous-map-player-and-ambient-traffic-prototype","iso":iso,"executable":executable,"elf_sha256":hash,
        "game_dat_bytes":game.len(),"iso_entries":disc.entries,"levels":levels.iter().map(|l|&l.stats).collect::<Vec<_>>(),
        "collision":collisions.stats,"ground_checks":ground_checks,"character":character.stats,"pose_checks":pose_checks,"vehicle":vehicle.stats,"water":water,"street_furniture":furniture,
        "limitations":["No missions or original game logic; pedestrian and traffic controllers are provisional","Banshee handling is approximate; native low-car entry/exit/root motion and door hinges, provisional alignment and opening timing","Original jump/swim poses and root height, controller-owned XY; provisional blends/physics, no gait IK","Native map/vehicle colliders and selected dynamic street props; mission-driven swaps remain incomplete","Visual visibility and HUD clock use noon; health/armor react to melee, economy not implemented","Native unarmed combat clips; damage, impact timing and retaliation remain provisional, no firearms/grappling", "Original PS2 preset disables optional modern crouch/low-ledge traversal; modern animation adaptation is not a native San Andreas animation", "LOD and additional interiors remain incomplete","Native water grid/texture and swimming clips; provisional swimmer buoyancy/drag, no waves/diving/stamina/quay climbing"]});
    std::fs::write(
        out.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Verification written to {}",
        out.join("verification.json").display()
    );
    if let Some(i) = args.iter().position(|s| s == "--inspect-map") {
        let position: Vec<f32> = args
            .get(i + 1)
            .ok_or("--inspect-map requires x,y,z")?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        if position.len() != 3 || position.iter().any(|v| !v.is_finite()) {
            return Err("Invalid map inspection position".into());
        }
        let report: Vec<_> = levels
            .iter()
            .map(|l| l.inspect_visibility([position[0], position[1], position[2]]))
            .collect();
        std::fs::write(
            out.join("map-inspection.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        return Ok(());
    }
    if args.iter().any(|s| s == "--audit-map") {
        let paths = paths::PathNetwork::load(&game)?;
        let audit = audit::map_visibility(&levels, &mut collisions, &paths);
        std::fs::write(
            out.join("map-visibility-audit.json"),
            serde_json::to_vec_pretty(&audit)?,
        )?;
        println!(
            "Map audit: {} samples, {} restored, {} remaining ground candidates",
            audit["ground_samples"],
            audit["restored_by_terrain_pass"],
            audit["remaining_ground_candidates"]
                .as_array()
                .map_or(0, |v| v.len())
        );
        return Ok(());
    }
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
        let mut preview_car = driving::Car::new(render::parked_vehicle(zone_index), -1.1);
        preview_car.settle_on_ground(&collisions, true, 0.0);
        let vehicle_frame = gpu::VehicleFrame {
            triangles: &vehicle.triangles,
            position: preview_car.position,
            heading: -1.1,
            health: 1000.0,
            burning: false,
            exploded: false,
            is_police: false,
            strobe_timer: 0.0,
            headlights: true,
        };
        frame.draw_scene_all(
            &levels,
            zone_index,
            &camera,
            true,
            figure.as_ref().map(|f| (f, &character.textures)),
            Some((&vehicle_frame, &vehicle.textures)),
        );
        hud::guide(
            &mut frame.pixels,
            scene::label(zone_index),
            walking,
            false,
            true,
        );
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
        let paths = paths::PathNetwork::load(&game)?;
        let npc_models: Vec<_> = [14, 19, 22]
            .into_iter()
            .map(|id| character::Character::load_model(&mut disc, &game, id))
            .collect::<Result<_>>()?;
        println!(
            "Ambient paths: {} car nodes, {} pedestrian nodes; {} original pedestrian models",
            paths.car_nodes,
            paths.nodes.len() - paths.car_nodes,
            npc_models.len()
        );
        std::fs::write(
            out.join("npc-models.json"),
            serde_json::to_vec_pretty(
                &npc_models
                    .iter()
                    .map(|model| &model.stats)
                    .collect::<Vec<_>>(),
            )?,
        )?;
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
        let simulate_jump = args.iter().any(|s| s == "--simulate-jump");
        let simulate_combat = args.iter().any(|s| s == "--simulate-combat");
        if simulate_combat
            && (!headless || simulate_seconds <= 0. || !args.iter().any(|s| s == "--walk"))
        {
            return Err("Combat simulation requires --headless --walk --simulate SECONDS".into());
        }
        let gameplay = match args.iter().position(|s| s == "--gameplay") {
            Some(i) => Some(match args.get(i + 1).map(String::as_str) {
                Some("original") => settings::Gameplay::OriginalPs2,
                Some("modern") => settings::Gameplay::Modern,
                _ => return Err("--gameplay expects original or modern".into()),
            }),
            None => None,
        };
        let simulate_vehicle = args
            .iter()
            .position(|s| s == "--simulate-vehicle")
            .map(|i| match args.get(i + 1).map(String::as_str) {
                Some("enter") => Ok("enter".to_string()),
                Some("exit") => Ok("exit".to_string()),
                _ => Err("--simulate-vehicle expects enter or exit"),
            })
            .transpose()?;
        let simulate_crouch = args.iter().any(|s| s == "--simulate-crouch");
        let simulate_vault = args.iter().any(|s| s == "--simulate-vault");
        if (simulate_vehicle.is_some() || simulate_crouch || simulate_vault)
            && (!headless || simulate_seconds <= 0.)
        {
            return Err("Movement simulation requires --headless --simulate SECONDS".into());
        }
        if (simulate_crouch || simulate_vault) && !args.iter().any(|s| s == "--walk") {
            return Err("Modern movement simulation requires --walk".into());
        }
        let capture_view = args
            .iter()
            .position(|s| s == "--view-angle")
            .map(|i| -> Result<[f32; 2]> {
                if !headless {
                    return Err("--view-angle requires --headless".into());
                }
                let values: Vec<f32> = args
                    .get(i + 1)
                    .ok_or("--view-angle needs yaw,pitch")?
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?;
                if values.len() != 2 || values.iter().any(|x| !x.is_finite()) {
                    return Err("--view-angle needs finite yaw,pitch".into());
                }
                Ok([values[0], values[1].clamp(-1.4, 1.4)])
            })
            .transpose()?;
        let simulate_motion = match args.iter().position(|s| s == "--simulate-motion") {
            Some(i) => {
                if !headless || simulate_seconds <= 0. {
                    return Err("Simulation input requires --headless --simulate SECONDS".into());
                }
                match args.get(i + 1).map(String::as_str) {
                    Some("idle") => 0,
                    Some("walk") => 1,
                    Some("fast") => 2,
                    _ => return Err("--simulate-motion expects idle, walk or fast".into()),
                }
            }
            None => 1,
        };
        if simulate_jump
            && (!headless || simulate_seconds <= 0. || !args.iter().any(|s| s == "--walk"))
        {
            return Err("Jump simulation requires --headless --walk --simulate SECONDS".into());
        }
        if simulate_seconds > 0.0 && !headless {
            return Err("Automatic simulation requires --headless".into());
        }
        let smoke = args.iter().any(|s| s == "--smoke") || capture.is_some() || headless;
        let mut objects = objects::Objects::new(
            &furniture.bindings,
            &levels,
            iso::f32le(&game, vehicle.stats.handling_record_address + 0xbc)?,
        )?;
        let game_hud = hud::GameHud::load(&mut disc, &game)?;
        let viewer_report = render::viewer(
            &mut levels,
            &mut collisions,
            &mut objects,
            &water_grid,
            &game_hud,
            &character,
            &vehicle,
            &npc_models,
            &paths,
            render::ViewerOptions {
                live_report: (!smoke).then(|| out.join("live-state.json")),
                smoke,
                initial: zone_index,
                start_walking: args.iter().any(|s| s == "--walk")
                    || (!smoke && !args.iter().any(|s| s == "--fly")),
                start_driving: args.iter().any(|s| s == "--drive"),
                software: args.iter().any(|s| s == "--software"),
                capture,
                headless,
                initial_camera: camera_override,
                third_person: !args.iter().any(|s| s == "--first-person"),
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
                show_options: args.iter().any(|s| s == "--show-options"),
                capture_view,
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
