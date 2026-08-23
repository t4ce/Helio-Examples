//! Headless build-time capture of Helio's real `SimpleCubePass` graph.
//!
//! This runs the normal Helio renderer on a native wgpu device with wgpu's
//! validated API trace enabled, then seals that trace in a versioned `.helio`
//! artifact. No cube geometry, shader, pipeline, or draw is recreated here.

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use glam::Vec3;
use helio::{
    Camera, DebugCameraUniform, DebugDrawState, Renderer, RendererConfig, Scene,
    required_experimental_features, required_wgpu_features, required_wgpu_limits,
};
use helio_artifact::{Artifact, Builder, DynamicSlot, Manifest, SectionKind};
use helio_default_graphs::build_simple_graph;

mod churn_scene;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let output = output_path()?;
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let trace_dir = parent.join(format!(".simple-cube-wgpu-trace-{}", std::process::id()));
    if trace_dir.exists() {
        return Err(format!(
            "temporary trace path already exists: {}",
            trace_dir.display()
        )
        .into());
    }
    fs::create_dir(&trace_dir)?;

    let adapter_name = capture_one_helio_frame(&trace_dir)?;
    let manifest = Manifest {
        schema: 1,
        engine: "Helio".into(),
        program: "simple-cube".into(),
        graph: "helio_default_graphs::build_simple_graph".into(),
        capture: "wgpu-native-trace-v30".into(),
        target_api: "trueos-render".into(),
        target_architecture: "intel-xe-lp".into(),
        surface_format: format!("{FORMAT:?}"),
        width: WIDTH,
        height: HEIGHT,
        dynamic_slots: vec![
            DynamicSlot {
                name: "camera.view_proj".into(),
                kind: "mat4x4-f32".into(),
            },
            DynamicSlot {
                name: "output.surface".into(),
                kind: "ui4-bgra8-srgb".into(),
            },
        ],
    };

    let mut builder = Builder::new(&manifest)?;
    builder.add(
        SectionKind::CompilerMetadata,
        "capture/adapter.txt",
        adapter_name.as_bytes().to_vec(),
    )?;
    let trace_files = collect_files(&trace_dir)?;
    if trace_files.is_empty() {
        return Err("wgpu produced an empty trace".into());
    }
    verify_simple_cube_trace(&trace_files)?;
    let trace_inputs: Vec<helio_artifact::TraceFile<'_>> = trace_files
        .iter()
        .map(|(name, data)| helio_artifact::TraceFile { name, data })
        .collect();
    let render_ir = helio_artifact::lower_simple_cube_wgpu_trace(&trace_inputs)?;
    builder.add_render_ir(&render_ir)?;
    builder.add(
        SectionKind::Other,
        churn_scene::SECTION_NAME,
        churn_scene::encode(),
    )?;
    for (relative, data) in trace_files {
        builder.add(
            SectionKind::WgpuTrace,
            format!("wgpu/{}", relative.replace('\\', "/")),
            data,
        )?;
    }

    let bytes = builder.finish()?;
    let checked = Artifact::parse(&bytes)?;
    let normalized = checked
        .section(helio_artifact::RENDER_IR_SECTION_NAME)
        .ok_or("artifact is missing normalized render IR")?;
    helio_artifact::RenderIrRef::parse(normalized.data)?;
    let section_count = checked.sections().count();
    fs::write(&output, bytes)?;
    fs::remove_dir_all(&trace_dir)?;

    println!(
        "baked {} ({} sections, {}x{}, adapter: {})",
        output.display(),
        section_count,
        WIDTH,
        HEIGHT,
        adapter_name
    );
    Ok(())
}

fn output_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args_os();
    let _program = args.next();
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/helio-artifacts/simple-cube.trueos.helio"));
    if args.next().is_some() {
        return Err("usage: bake_simple_cube [output.helio]".into());
    }
    Ok(output)
}

fn capture_one_helio_frame(trace_dir: &Path) -> Result<String, Box<dyn Error>> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let adapter_info = adapter.get_info();
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("Helio TRUEOS build-time capture"),
        required_features: required_wgpu_features(adapter.features()),
        required_limits: required_wgpu_limits(adapter.limits()),
        experimental_features: required_experimental_features(adapter.features()),
        trace: wgpu::Trace::Directory(trace_dir.to_path_buf()),
        ..Default::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&descriptor))?;
    let device = Arc::new(device);
    let queue = Arc::new(queue);

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Helio TRUEOS artifact output surface"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let config = RendererConfig::new(WIDTH, HEIGHT, FORMAT);
    let scene = Scene::new(device.clone(), queue.clone());
    let debug_camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Debug Camera Buffer"),
        size: std::mem::size_of::<DebugCameraUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cull_stats_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Cull Stats Buffer"),
        size: 32,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let debug_state = Arc::new(std::sync::Mutex::new(DebugDrawState::default()));
    let mut graph = build_simple_graph(&device, &queue, FORMAT);
    graph.lock(WIDTH, HEIGHT);
    let mut renderer = Renderer::new(
        device.clone(),
        queue.clone(),
        config.surface_format,
        config.width,
        config.height,
        config.render_scale,
        config,
        scene,
        graph,
        debug_state,
        debug_camera_buf,
        cull_stats_buf,
    );

    let camera = Camera::perspective_look_at(
        Vec3::new(0.0, 0.8, 4.0),
        Vec3::ZERO,
        Vec3::Y,
        std::f32::consts::FRAC_PI_4,
        WIDTH as f32 / HEIGHT as f32,
        0.01,
        100.0,
    );
    renderer.render(&camera, &target_view)?;
    device.poll(wgpu::PollType::wait_indefinitely())?;

    drop(renderer);
    drop(target_view);
    drop(target);
    drop(queue);
    drop(device);
    drop(adapter);
    drop(instance);

    Ok(format!(
        "name={}\nbackend={:?}\ndevice_type={:?}\n",
        adapter_info.name, adapter_info.backend, adapter_info.device_type
    ))
}

fn collect_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>, Box<dyn Error>> {
    fn visit(
        root: &Path,
        current: &Path,
        output: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), Box<dyn Error>> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                visit(root, &path, output)?;
            } else if file_type.is_file() {
                let relative = path.strip_prefix(root)?.to_string_lossy().into_owned();
                output.push((relative, fs::read(path)?));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn verify_simple_cube_trace(files: &[(String, Vec<u8>)]) -> Result<(), Box<dyn Error>> {
    let trace = files
        .iter()
        .find(|(name, _)| name == "trace.ron")
        .ok_or("wgpu trace has no trace.ron")?;
    let trace = std::str::from_utf8(&trace.1)?;
    for required in [
        "SimpleCube Shader",
        "SimpleCube Pipeline",
        "SimpleCube VB",
        "SimpleCube IB",
        "vs_main",
        "fs_main",
        "DrawIndexed(",
        "index_count: 36",
        "instance_count: 1",
        "size: Some(864)",
        "size: Some(72)",
    ] {
        if !trace.contains(required) {
            return Err(
                format!("wgpu trace is missing required SimpleCube event: {required}").into(),
            );
        }
    }
    Ok(())
}
