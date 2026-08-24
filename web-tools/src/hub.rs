//! Lightweight egui launcher for the Helio examples workspace.

use std::{path::PathBuf, process::Command};

struct Entry {
    label: &'static str,
    package: &'static str,
    bin: &'static str,
    args: &'static [&'static str],
}

macro_rules! demo {
    ($label:literal, $bin:literal) => {
        Entry {
            label: $label,
            package: "examples",
            bin: $bin,
            args: &[],
        }
    };
}

const ENTRIES: &[Entry] = &[
    Entry {
        label: "Build and serve web demos",
        package: "helio-web-tools",
        bin: "web",
        args: &[],
    },
    Entry {
        label: "Build web demos (headless)",
        package: "helio-web-tools",
        bin: "web",
        args: &["--headless"],
    },
    demo!("Bake simple cube artifact", "bake_simple_cube"),
    demo!("HelioV voxel flycam", "heliov_flycam"),
    Entry {
        label: "Cloud Engine",
        package: "examples",
        bin: "cloud_engine",
        args: &["--preset", "porcelain"],
    },
    demo!("Voxel mesh world", "voxel_demo"),
    demo!("Voxel raymarch world", "voxel_demo_raymarch"),
    demo!("Planet voxel", "planet_voxel_demo"),
    demo!("Portal showcase", "portals_demo"),
    demo!("Portal rooms", "portal_rooms"),
    demo!("Corona particles", "corona_demo"),
    demo!("Radiant materials", "radiant_demo"),
    demo!("Planar reflections", "planar_reflection_demo"),
    demo!("Volumetric fog", "volumetric_fog_demo"),
    demo!("Rapier pendulum", "rapier_pendulum"),
    demo!("Rapier stack", "rapier_stack"),
    demo!("Shape battle royale", "shape_battle_royale"),
    demo!("Churn benchmark", "churn_benchmark"),
    demo!("One million cubes", "one_million_cubes"),
    demo!("Indoor cathedral", "indoor_cathedral"),
    demo!("Outdoor city", "outdoor_city"),
    demo!("Foliage", "foliage_demo"),
    demo!("Editor", "editor_demo"),
    demo!("VR desktop mirror", "vr_demo"),
];

struct Hub {
    examples_root: PathBuf,
    status: String,
}

impl Hub {
    fn launch(&mut self, entry: &Entry) {
        let mut command = Command::new("cargo");
        command
            .args(["run", "--manifest-path"])
            .arg(self.examples_root.join("Cargo.toml"))
            .args(["-p", entry.package, "--bin", entry.bin]);
        if !entry.args.is_empty() {
            command.arg("--").args(entry.args);
        }
        self.status = match command.spawn() {
            Ok(_) => format!("Started {}", entry.label),
            Err(error) => format!("Could not start {}: {error}", entry.label),
        };
    }
}

impl eframe::App for Hub {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _: &mut eframe::Frame) {
        ui.heading("Helio Examples");
        ui.label("Choose a demo or build action.");
        ui.separator();
        eframe::egui::ScrollArea::vertical().show(ui, |ui| {
            for entry in ENTRIES {
                if ui.button(entry.label).clicked() {
                    self.launch(entry);
                }
            }
        });
        ui.separator();
        ui.small(&self.status);
    }
}

fn main() -> eframe::Result {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_root = if manifest_dir
        .file_name()
        .is_some_and(|name| name == "web-tools")
    {
        manifest_dir.parent().unwrap().to_path_buf()
    } else {
        manifest_dir
    };
    eframe::run_native(
        "Helio Examples",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([420.0, 620.0])
                .with_min_inner_size([320.0, 400.0]),
            ..Default::default()
        },
        Box::new(move |_| {
            Ok(Box::new(Hub {
                examples_root,
                status: "Ready".into(),
            }))
        }),
    )
}
