//! Helio Examples Hub.
//!
//! `cargo run` opens this compact native control window and starts the HelioV
//! voxel demo beside it. Selecting an item launches that example from the same
//! examples workspace, so individual bin names no longer need to be memorized.

use std::path::PathBuf;
use std::process::Command;

const DEMOS: &[(&str, &str)] = &[
    ("HelioV voxel flycam", "heliov_flycam"),
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

struct ExamplesHub {
    workspace: PathBuf,
    started_voxel_demo: bool,
    status: String,
}

impl ExamplesHub {
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
}

impl eframe::App for ExamplesHub {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _: &mut eframe::Frame) {
        if !self.started_voxel_demo {
            self.started_voxel_demo = true;
            self.launch("heliov_flycam");
        }

        ui.heading("Helio Examples");
        ui.label("The voxel flycam runs beside this control window.");
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
    let workspace = std::env::current_dir().expect("examples working directory");
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
                started_voxel_demo: false,
                status: "Starting HelioV voxel flycam…".to_owned(),
            }))
        }),
    )
}
