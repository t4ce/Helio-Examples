//! Lightweight terminal launcher for the Helio examples workspace.

use std::{io, path::PathBuf, process::Command, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};

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
        label: "Web: build and serve all demos",
        package: "helio-web-tools",
        bin: "web",
        args: &[],
    },
    Entry {
        label: "Web: headless build",
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

fn select_entry() -> io::Result<Option<&'static Entry>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut state = ListState::default().with_selected(Some(0));

    let result = loop {
        terminal.draw(|frame| {
            let list = List::new(ENTRIES.iter().map(|entry| ListItem::new(entry.label)))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Helio Examples — Enter launch · q quit "),
                )
                .highlight_symbol("▶ ")
                .highlight_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
            frame.render_stateful_widget(list, frame.area(), &mut state);
        })?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let selected = state.selected().unwrap_or(0);
                match key.code {
                    KeyCode::Up => state.select(Some(selected.saturating_sub(1))),
                    KeyCode::Down => state.select(Some((selected + 1).min(ENTRIES.len() - 1))),
                    KeyCode::Enter => break Some(&ENTRIES[selected]),
                    KeyCode::Char('q') | KeyCode::Esc => break None,
                    _ => {}
                }
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(result)
}

fn main() -> io::Result<()> {
    let Some(entry) = select_entry()? else {
        return Ok(());
    };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_root = if manifest_dir
        .file_name()
        .is_some_and(|name| name == "web-tools")
    {
        manifest_dir
            .parent()
            .expect("web-tools must live under Helio-Examples")
            .to_path_buf()
    } else {
        manifest_dir
    };
    let mut command = Command::new("cargo");
    command
        .args(["run", "--manifest-path"])
        .arg(examples_root.join("Cargo.toml"))
        .args(["-p", entry.package, "--bin", entry.bin]);
    if !entry.args.is_empty() {
        command.arg("--").args(entry.args);
    }
    let status = command.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
