//! Helio Examples Hub.
//!
//! `cargo run` opens this compact native control window.
//! Selecting an item launches that example from the same examples workspace, so
//! individual bin names no longer need to be memorized.

use std::path::PathBuf;
use std::process::Command;

const DEMOS: &[(&str, &str)] = &[
    ("HelioV voxel flycam", "heliov_flycam"),
    ("Cloud Engine", "cloud_engine"),
    ("Voxel mesh world", "voxel_demo"),
    ("Voxel raymarch world", "voxel_demo_raymarch"),
    ("Planet voxel", "planet_voxel_demo"),
    ("Portal showcase", "portals_demo"),
    ("Portal rooms", "portal_rooms"),
    ("Corona particles", "corona_demo"),
    ("Radiant materials", "radiant_demo"),
    ("Planar reflections", "planar_reflection_demo"),
    ("Volumetric fog", "volumetric_fog_demo"),
    ("Rapier pendulum", "rapier_pendulum"),
    ("Rapier stack", "rapier_stack"),
    ("Shape battle royale", "shape_battle_royale"),
    ("Churn benchmark", "churn_benchmark"),
    ("One million cubes", "one_million_cubes"),
    ("Indoor cathedral", "indoor_cathedral"),
    ("Outdoor city", "outdoor_city"),
    ("Foliage", "foliage_demo"),
    ("Editor", "editor_demo"),
    ("VR desktop mirror", "vr_demo"),
];

const CLOUD_PRESETS: &[(&str, &str)] = &[
    ("Verdant moon", "verdant"),
    ("Blue porcelain", "porcelain"),
    ("Amber dusk", "ember"),
    ("Violet night", "violet"),
];

struct ExamplesHub {
    workspace: PathBuf,
    status: String,
}

impl ExamplesHub {
    fn cargo_run(&mut self, bin: &str, args: &[&str]) {
        let mut command = Command::new("cargo");
        command
            .args(["run", "--manifest-path"])
            .arg(self.workspace.join("Cargo.toml"))
            .args(["--bin", bin]);
        if !args.is_empty() {
            command.arg("--").args(args);
        }

        self.status = match command.spawn() {
            Ok(_) => format!("Started {bin}"),
            Err(error) => format!("Could not start {bin}: {error}"),
        };
    }

    fn launch(&mut self, bin: &str) {
        let executable = self.workspace.join("target").join("debug").join(bin);
        let result = if executable.is_file() {
            Command::new(&executable).spawn()
        } else {
            Command::new("cargo")
                .args(["run", "--bin", bin])
                .current_dir(&self.workspace)
                .spawn()
        };

        self.status = match result {
            Ok(_) => format!("Started {bin}"),
            Err(error) => format!("Could not start {bin}: {error}"),
        };
    }

    fn launch_with_args(&mut self, bin: &str, args: &[&str]) {
        let executable = self.workspace.join("target").join("debug").join(bin);
        let result = if executable.is_file() {
            Command::new(&executable).args(args).spawn()
        } else {
            let mut command = Command::new("cargo");
            command.args(["run", "--bin", bin]);
            if !args.is_empty() {
                command.arg("--");
            }
            command.args(args);
            command.current_dir(&self.workspace).spawn()
        };

        self.status = match result {
            Ok(_) => format!("Started {bin}"),
            Err(error) => format!("Could not start {bin}: {error}"),
        };
    }
}

impl eframe::App for ExamplesHub {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _: &mut eframe::Frame) {
        ui.heading("Helio Examples");
        ui.label("Nothing auto-starts. Pick a demo to launch.");
        ui.separator();

        ui.collapsing("Cloud Engine presets (bypass settings window)", |ui| {
            if ui.button("Launch Cloud Engine (default)").clicked() {
                self.launch_with_args("cloud_engine", &["--preset", "porcelain"]);
            }
            for &(label, preset) in CLOUD_PRESETS {
                if ui
                    .button(format!("Launch Cloud Engine — {label}"))
                    .clicked()
                {
                    self.launch_with_args("cloud_engine", &["--preset", preset]);
                }
            }
        });

        ui.separator();

        ui.collapsing("Build & utility actions", |ui| {
            if ui.button("Build & serve all web demos").clicked() {
                self.cargo_run("web", &[]);
            }
            if ui.button("Build web demos (headless)").clicked() {
                self.cargo_run("web", &["--headless"]);
            }
            if ui.button("Bake simple cube artifact").clicked() {
                self.launch("bake_simple_cube");
            }
        });

        ui.separator();
        eframe::egui::ScrollArea::vertical().show(ui, |ui| {
            for &(label, bin) in DEMOS {
                if ui.button(label).clicked() {
                    self.launch(bin);
                }
            }
        });
        ui.separator();
        ui.small(&self.status);
    }
}

fn main() -> eframe::Result {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    eframe::run_native(
        "Helio Examples",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([300.0, 560.0])
                .with_min_inner_size([260.0, 360.0]),
            ..Default::default()
        },
        Box::new(move |_| {
            Ok(Box::new(ExamplesHub {
                workspace,
                status: "Ready.".to_owned(),
            }))
        }),
    )
}
