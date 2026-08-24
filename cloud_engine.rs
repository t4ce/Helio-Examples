//! Cloud Engine — native Helio example.
//!
//! The artistic preset from `cloud-engine-config_2026-08-23_06-14-45.json` is
//! deliberately compiled into this viewport-only port. There is no control UI:
//! left-drag paints clouds and right-drag erases them. Press F for the standard
//! HelioV flycam (WASD, Space/Shift, IJKL, and mouse look); Escape releases the
//! cursor and returns to painting, then Escape again closes the example.

use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use glam::Vec3;
use microfont::{stamp_text, FHEIGHT};
use helio::{FlyCamera, FlyCameraConfig};
use helio_controls::WinitFlyInput;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const VOLUME: (u32, u32, u32) = (96, 48, 96);
// Keep simulation/painting in the original cloudbox but draw a flatter box in the
// viewport for coverage.
const BOUNDS_XZ_EXPAND_FACTOR: f32 = 2.0;
const BOUNDS_Y_HEIGHT_SCALE: f32 = 0.5;
const BOUNDS_SIM_MIN_X: f32 = -8.0;
const BOUNDS_SIM_MAX_X: f32 = 8.0;
const BOUNDS_SIM_MIN_Z: f32 = 8.5 - (9.5 * BOUNDS_XZ_EXPAND_FACTOR);
const BOUNDS_SIM_MAX_Z: f32 = 8.5 + (9.5 * BOUNDS_XZ_EXPAND_FACTOR);
const BOUNDS_SIM_MIN_Y: f32 = -2.0;
const BOUNDS_SIM_MAX_Y: f32 = 9.0;

const BOUNDS_SIM_MIN: Vec3 = Vec3::new(
    BOUNDS_SIM_MIN_X * BOUNDS_XZ_EXPAND_FACTOR,
    BOUNDS_SIM_MIN_Y,
    BOUNDS_SIM_MIN_Z,
);
const BOUNDS_SIM_MAX: Vec3 = Vec3::new(
    BOUNDS_SIM_MAX_X * BOUNDS_XZ_EXPAND_FACTOR,
    BOUNDS_SIM_MAX_Y,
    BOUNDS_SIM_MAX_Z,
);

const BOUNDS_RENDER_Y_MID: f32 = (BOUNDS_SIM_MIN_Y + BOUNDS_SIM_MAX_Y) * 0.5;
const BOUNDS_RENDER_Y_HALF: f32 = ((BOUNDS_SIM_MAX_Y - BOUNDS_SIM_MIN_Y) * 0.5) * BOUNDS_Y_HEIGHT_SCALE;
const BOUNDS_RENDER_MIN: Vec3 = Vec3::new(
    BOUNDS_SIM_MIN_X * BOUNDS_XZ_EXPAND_FACTOR,
    BOUNDS_RENDER_Y_MID - BOUNDS_RENDER_Y_HALF,
    BOUNDS_SIM_MIN_Z,
);
const BOUNDS_RENDER_MAX: Vec3 = Vec3::new(
    BOUNDS_SIM_MAX_X * BOUNDS_XZ_EXPAND_FACTOR,
    BOUNDS_RENDER_Y_MID + BOUNDS_RENDER_Y_HALF,
    BOUNDS_SIM_MAX_Z,
);
const SIMULATION_INTERVAL: f32 = 1.0 / 30.0;

// Static version of cloud-engine-config_2026-08-23_06-14-45.json. The saved
// document used Draw mode, whose mutable painted voxels are intentionally not
// part of JSON export. This native port preserves that interaction: it begins
// with an empty editable volume.
const PRESET_POSITION: Vec3 = Vec3::new(-1.1756307, 1.5082186, -6.268016);
const PRESET_YAW: f32 = 0.0;
const PRESET_PITCH: f32 = 0.084796175;
const PRESET_FOV_DEGREES: f32 = 61.0;
const PRESET_SEED: f32 = 19.37;
// A restrained shared daylight cue: each endpoint is reached linearly, then
// the direction reverses, avoiding a visible snap at the loop boundary.
const DAYLIGHT_CYCLE_SECONDS: f32 = 120.0;
const SUN_ELEVATION_MIN_DEGREES: f32 = -6.0;
const SUN_ELEVATION_MAX_DEGREES: f32 = 8.0;
const SUN_AZIMUTH_DEGREES: f32 = 18.0;
const EXPOSURE_MIN: f32 = 1.20;
const EXPOSURE_MAX: f32 = 1.48;
const BRUSH_SIZE_MIN: f32 = 0.035;
const BRUSH_SIZE_MAX: f32 = 0.125;
const BRUSH_DRAIN_PER_SECOND: f32 = 0.016;
const BRUSH_RECHARGE_PER_SECOND: f32 = 0.090;
const WIND_SPEED_RADIANS_PER_SECOND: f32 = std::f32::consts::TAU / 180.0;
const WIND_INITIAL_ANGLE_RADIANS: f32 = 5.0_f32.to_radians();
const WIND_STRENGTH: f32 = 1.95;
const PRESET_CYCLE_SECONDS: f32 = 120.0;
const PRESET_CYCLE_COUNT: f32 = 4.0;
const CLOUDBOX_OUTLINE_COLOR: [f32; 4] = [0.16, 0.58, 1.0, 0.92];
const CLOUDBOX_OUTLINE_EPS: f32 = 0.001;
const PERF_MODES: [(f32, f32); 6] = [
    (70.0, 0.0),
    (50.0, 0.0),
    (30.0, 0.0),
    (70.0, 1.0),
    (50.0, 1.0),
    (30.0, 1.0),
];

const PERF_OVERLAY_TEXTURE_WIDTH: u32 = 192;
const PERF_OVERLAY_TEXTURE_HEIGHT: u32 = 32;
const PERF_OVERLAY_SCALE: f32 = 2.0;
const PERF_OVERLAY_MARGIN: f32 = 12.0;
const PERF_OVERLAY_SHADER: &str = r#"
struct Rect { ndc: vec4<f32> }
@group(0) @binding(0) var font_tex: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(2) var<uniform> rect: Rect;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    var out: VertexOut;
    out.position = vec4<f32>(mix(rect.ndc.xy, rect.ndc.zw, corner), 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(font_tex, font_sampler, input.uv);
}
"#;

const SIM_SHADER: &str = include_str!("cloud-engine-webgpu-linux-aligned/shaders/simulate.wgsl");
const RENDER_SHADER: &str = include_str!("cloud-engine-webgpu-linux-aligned/shaders/render.wgsl");
const OUTLINE_SHADER: &str = include_str!("cloud-engine-outline.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtPreset {
    Verdant,
    Porcelain,
    Ember,
    Violet,
}

#[derive(Clone, Copy)]
enum PresetMode {
    Static(ArtPreset),
    Cycle,
}

#[derive(Clone, Copy)]
struct ArtDirection {
    cloud: Vec3,
    shadow: Vec3,
    sky: Vec3,
    moon: Vec3,
    curl: f32,
    ribbon: f32,
    sculpt: f32,
    bands: f32,
    outline: f32,
    moon_size: f32,
    moon_glow: f32,
    grain: f32,
}

impl ArtPreset {
    const fn direction(self) -> ArtDirection {
        match self {
            Self::Verdant => ArtDirection {
                cloud: Vec3::new(0.038, 0.297, 0.287),
                shadow: Vec3::new(0.002, 0.093, 0.086),
                sky: Vec3::new(0.002, 0.05, 0.054),
                moon: Vec3::new(1.0, 0.807, 0.175),
                curl: 1.10,
                ribbon: 0.72,
                sculpt: 0.78,
                bands: 5.0,
                outline: 0.62,
                moon_size: 0.18,
                moon_glow: 1.15,
                grain: 0.20,
            },
            Self::Porcelain => ArtDirection {
                cloud: Vec3::new(0.168, 0.479, 0.584),
                shadow: Vec3::new(0.009, 0.045, 0.095),
                sky: Vec3::new(0.0033, 0.018, 0.06),
                moon: Vec3::new(1.0, 0.871, 0.402),
                curl: 1.28,
                ribbon: 0.86,
                sculpt: 0.84,
                bands: 6.0,
                outline: 0.76,
                moon_size: 0.155,
                moon_glow: 0.92,
                grain: 0.13,
            },
            Self::Ember => ArtDirection {
                cloud: Vec3::new(0.694, 0.258, 0.098),
                shadow: Vec3::new(0.105, 0.022, 0.038),
                sky: Vec3::new(0.026, 0.01, 0.036),
                moon: Vec3::new(1.0, 0.658, 0.15),
                curl: 0.96,
                ribbon: 0.64,
                sculpt: 0.72,
                bands: 4.0,
                outline: 0.54,
                moon_size: 0.21,
                moon_glow: 1.52,
                grain: 0.28,
            },
            Self::Violet => ArtDirection {
                cloud: Vec3::new(0.254, 0.191, 0.571),
                shadow: Vec3::new(0.029, 0.022, 0.078),
                sky: Vec3::new(0.009, 0.009, 0.029),
                moon: Vec3::new(0.922, 0.716, 0.287),
                curl: 1.42,
                ribbon: 0.94,
                sculpt: 0.82,
                bands: 5.0,
                outline: 0.88,
                moon_size: 0.145,
                moon_glow: 1.05,
                grain: 0.24,
            },
        }
    }

    fn parse_arg(name: &str) -> Option<Self> {
        match name {
            "verdant" => Some(Self::Verdant),
            "porcelain" => Some(Self::Porcelain),
            "ember" => Some(Self::Ember),
            "violet" => Some(Self::Violet),
            _ => None,
        }
    }

    fn from_index(index: usize) -> Self {
        match index % 4 {
            0 => Self::Verdant,
            1 => Self::Porcelain,
            2 => Self::Ember,
            _ => Self::Violet,
        }
    }

    fn lerp(a: Self, b: Self, t: f32) -> ArtDirection {
        let from = a.direction();
        let to = b.direction();
        let mt = t.clamp(0.0, 1.0);
        let one_minus = 1.0 - mt;
        ArtDirection {
            cloud: from.cloud * one_minus + to.cloud * mt,
            shadow: from.shadow * one_minus + to.shadow * mt,
            sky: from.sky * one_minus + to.sky * mt,
            moon: from.moon * one_minus + to.moon * mt,
            curl: from.curl * one_minus + to.curl * mt,
            ribbon: from.ribbon * one_minus + to.ribbon * mt,
            sculpt: from.sculpt * one_minus + to.sculpt * mt,
            bands: from.bands * one_minus + to.bands * mt,
            outline: from.outline * one_minus + to.outline * mt,
            moon_size: from.moon_size * one_minus + to.moon_size * mt,
            moon_glow: from.moon_glow * one_minus + to.moon_glow * mt,
            grain: from.grain * one_minus + to.grain * mt,
        }
    }
}

impl PresetMode {
    fn to_art_direction(self, time: f32) -> ArtDirection {
        match self {
            Self::Static(preset) => preset.direction(),
            Self::Cycle => {
                let cycle_time = (time / PRESET_CYCLE_SECONDS).fract();
                let segments = PRESET_CYCLE_COUNT;
                let segment_t = cycle_time * segments;
                let from_index = segment_t.floor() as usize;
                let local_t = segment_t - from_index as f32;
                let to_index = (from_index + 1) % 4;
                ArtPreset::lerp(
                    ArtPreset::from_index(from_index),
                    ArtPreset::from_index(to_index),
                    local_t,
                )
            }
        }
    }
}

struct PerfCycleMode {
    steps: u32,
    detail_tier: u32,
}

impl PerfCycleMode {
    const fn from_index(index: u32) -> Self {
        let (steps, tier) = PERF_MODES[(index as usize) % PERF_MODES.len()];
        Self {
            steps: steps as u32,
            detail_tier: tier as u32,
        }
    }
}

struct PerfOverlay {
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    pixels: Vec<u32>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    rect_buffer: wgpu::Buffer,
    frame_accum: f32,
    frame_count: u32,
    last_fps: u32,
}

impl PerfOverlay {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cloud Engine Perf overlay texture"),
            size: wgpu::Extent3d {
                width: PERF_OVERLAY_TEXTURE_WIDTH,
                height: PERF_OVERLAY_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud Engine Perf overlay sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cloud Engine Perf overlay BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud Engine Perf overlay rect"),
            size: std::mem::size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud Engine Perf overlay BG"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: rect_buffer.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cloud Engine Perf overlay PL"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud Engine Perf overlay WGSL"),
            source: wgpu::ShaderSource::Wgsl(PERF_OVERLAY_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Engine Perf overlay pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mut overlay = Self {
            _texture: texture,
            _view: view,
            pixels: vec![0; (PERF_OVERLAY_TEXTURE_WIDTH * PERF_OVERLAY_TEXTURE_HEIGHT) as usize],
            pipeline,
            bind_group,
            rect_buffer,
            frame_accum: 0.0,
            frame_count: 0,
            last_fps: 0,
        };
        overlay.update_text(queue, 0, 0);
        overlay
    }

    fn overlay_rect_ndc(width: u32, height: u32) -> [f32; 4] {
        let overlay_w = PERF_OVERLAY_TEXTURE_WIDTH as f32 * PERF_OVERLAY_SCALE;
        let overlay_h = PERF_OVERLAY_TEXTURE_HEIGHT as f32 * PERF_OVERLAY_SCALE;
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let left = width - PERF_OVERLAY_MARGIN - overlay_w;
        let right = width - PERF_OVERLAY_MARGIN;
        let top = height - PERF_OVERLAY_MARGIN - overlay_h;
        let bottom = height - PERF_OVERLAY_MARGIN;
        let left_ndc = left * 2.0 / width - 1.0;
        let right_ndc = right * 2.0 / width - 1.0;
        let top_ndc = -1.0 + 2.0 * top / height;
        let bottom_ndc = -1.0 + 2.0 * bottom / height;
        [left_ndc, top_ndc, right_ndc, bottom_ndc]
    }

    fn is_click_within(position: winit::dpi::PhysicalPosition<f64>, width: u32, height: u32) -> bool {
        let overlay_w = PERF_OVERLAY_TEXTURE_WIDTH as f64 * PERF_OVERLAY_SCALE as f64;
        let overlay_h = PERF_OVERLAY_TEXTURE_HEIGHT as f64 * PERF_OVERLAY_SCALE as f64;
        let left = f64::from(width).max(1.0) - PERF_OVERLAY_MARGIN as f64 - overlay_w;
        let right = f64::from(width).max(1.0) - PERF_OVERLAY_MARGIN as f64;
        let top = f64::from(height).max(1.0) - PERF_OVERLAY_MARGIN as f64 - overlay_h;
        let bottom = f64::from(height).max(1.0) - PERF_OVERLAY_MARGIN as f64;
        position.x >= left && position.x <= right && position.y >= top && position.y <= bottom
    }

    fn update_text(&mut self, queue: &wgpu::Queue, fps: u32, mode_index: u32) {
        let mode = PerfCycleMode::from_index(mode_index);
        let text = format!("FPS {:>3}  steps {:>2}  tier {}", fps, mode.steps, mode.detail_tier);
        self.pixels.fill(0);
        stamp_text(
            &mut self.pixels,
            PERF_OVERLAY_TEXTURE_WIDTH as usize,
            PERF_OVERLAY_TEXTURE_HEIGHT as usize,
            4,
            ((PERF_OVERLAY_TEXTURE_HEIGHT as usize - FHEIGHT as usize) / 2) as i32,
            &text,
            u32::MAX,
        )
        .expect("Cloud engine perf overlay texture dimensions");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(PERF_OVERLAY_TEXTURE_WIDTH * 4),
                rows_per_image: Some(PERF_OVERLAY_TEXTURE_HEIGHT),
            },
            wgpu::Extent3d {
                width: PERF_OVERLAY_TEXTURE_WIDTH,
                height: PERF_OVERLAY_TEXTURE_HEIGHT,
                depth_or_array_layers: 1,
            },
        );
    }

    fn draw(
        &mut self,
        dt: f32,
        mode_index: u32,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        output_size: winit::dpi::PhysicalSize<u32>,
    ) {
        self.frame_accum += dt.max(0.0);
        self.frame_count = self.frame_count.saturating_add(1);
        if self.frame_accum >= 0.25 {
            self.last_fps = (self.frame_count as f32 / self.frame_accum).round() as u32;
            self.frame_accum = 0.0;
            self.frame_count = 0;
            self.update_text(queue, self.last_fps, mode_index);
        }
        let rect = Self::overlay_rect_ndc(output_size.width, output_size.height);
        queue.write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&rect));
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })];
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cloud Engine perf overlay"),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

fn main() {
    env_logger::init();
    let mut preset_override = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--preset" {
            if let Some(value) = args.next() {
                preset_override = ArtPreset::parse_arg(value.as_str());
            }
        }
    }

    let start_preset = preset_override
        .map(PresetMode::Static)
        .unwrap_or(PresetMode::Cycle);

    EventLoop::new()
        .expect("event loop")
        .run_app(&mut App::new(start_preset))
        .expect("event loop error");
}

struct App {
    state: Option<State>,
    art_preset: Arc<Mutex<PresetMode>>,
}

impl App {
    fn new(start_preset: PresetMode) -> Self {
        Self {
            state: None,
            art_preset: Arc::new(Mutex::new(start_preset)),
        }
    }
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    config: wgpu::SurfaceConfiguration,
    compute_pipeline: wgpu::ComputePipeline,
    render_pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    sim_uniform: wgpu::Buffer,
    render_uniform: wgpu::Buffer,
    outline_uniform: wgpu::Buffer,
    outline_bind_group: wgpu::BindGroup,
    sim_groups: [wgpu::BindGroup; 2],
    render_groups: [wgpu::BindGroup; 2],
    outline_vertex_buffer: wgpu::Buffer,
    outline_vertex_count: u32,
    current_volume: usize,
    camera: FlyCamera,
    input: WinitFlyInput,
    pointer_position: Option<winit::dpi::PhysicalPosition<f64>>,
    brush_center: Vec3,
    brush_active: bool,
    brush_sign: f32,
    brush_size: f32,
    art_preset: Arc<Mutex<PresetMode>>,
    perf_mode_index: u32,
    last_frame: Instant,
    simulation_accumulator: f32,
    time: f32,
    frame: u32,
    pending_clear: bool,
    perf_overlay: PerfOverlay,
}

impl App {
    fn create_state(event_loop: &ActiveEventLoop, art_preset: Arc<Mutex<PresetMode>>) -> State {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Helio — Cloud Engine")
                        .with_inner_size(winit::dpi::PhysicalSize::new(1600, 900)),
                )
                .expect("create Cloud Engine window"),
        );
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: true,
        }))
        .expect("GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: helio::required_wgpu_features(adapter.features()),
            required_limits: helio::required_wgpu_limits(adapter.limits()),
            experimental_features: helio::required_experimental_features(adapter.features()),
            ..Default::default()
        }))
        .expect("GPU device");
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let sim_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud simulation parameters"),
            size: (28 * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let render_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud render parameters"),
            size: (68 * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sim_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud simulation WGSL"),
            source: wgpu::ShaderSource::Wgsl(SIM_SHADER.into()),
        });
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud raymarch WGSL"),
            source: wgpu::ShaderSource::Wgsl(RENDER_SHADER.into()),
        });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Cloud density simulation"),
            layout: None,
            module: &sim_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud raymarch"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let outline_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloudbox outline WGSL"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_SHADER.into()),
        });
        let outline_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloudbox outline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &outline_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &outline_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let outline_vertex_data: Vec<f32> = [
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MIN.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MAX.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MIN.z,
            BOUNDS_RENDER_MIN.x, BOUNDS_RENDER_MAX.y, BOUNDS_RENDER_MAX.z,
        ]
        .to_vec();

        let outline_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloudbox outline vertices"),
            size: (outline_vertex_data.len() * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let outline_vertex_count = (outline_vertex_data.len() / 3) as u32;

        let outline_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloudbox outline uniforms"),
            size: (24 * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let outline_bind_group = {
            let layout = outline_pipeline.get_bind_group_layout(0);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Cloudbox outline bindings"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: outline_uniform.as_entire_binding(),
                }],
            })
        };
        queue.write_buffer(
            &outline_vertex_buffer,
            0,
            bytemuck::cast_slice(&outline_vertex_data),
        );
        let make_volume = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: VOLUME.0,
                    height: VOLUME.1,
                    depth_or_array_layers: VOLUME.2,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            })
        };
        let volumes = [make_volume("Cloud volume A"), make_volume("Cloud volume B")];
        let views = [
            volumes[0].create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D3),
                ..Default::default()
            }),
            volumes[1].create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D3),
                ..Default::default()
            }),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cloud trilinear sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sim_layout = compute_pipeline.get_bind_group_layout(0);
        let sim_groups = std::array::from_fn(|source| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Cloud simulation bindings"),
                layout: &sim_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: sim_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&views[source]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&views[1 - source]),
                    },
                ],
            })
        });
        let render_layout = render_pipeline.get_bind_group_layout(0);
        let render_groups = std::array::from_fn(|volume| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Cloud render bindings"),
                layout: &render_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: render_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&views[volume]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        });
        let perf_overlay = PerfOverlay::new(&device, &queue, config.format);
        State {
            window,
            surface,
            device: device.clone(),
            queue: queue.clone(),
            config,
            compute_pipeline,
            render_pipeline,
            sim_uniform,
            render_uniform,
            outline_uniform,
            outline_bind_group,
            sim_groups,
            render_groups,
            outline_pipeline,
            outline_vertex_buffer,
            outline_vertex_count,
            current_volume: 0,
            // The WebGPU source used +Z as forward. Helio's standard flycam
            // uses -Z, so rotate this imported pose by π to preserve its view.
            camera: FlyCamera::new(
                PRESET_POSITION,
                PRESET_YAW + std::f32::consts::PI,
                PRESET_PITCH,
                FlyCameraConfig::default(),
            ),
            input: WinitFlyInput::new(),
            pointer_position: None,
            brush_center: Vec3::new(0.5, 0.45, 0.64),
            brush_active: false,
            brush_sign: 1.0,
            brush_size: BRUSH_SIZE_MAX,
            art_preset,
            perf_mode_index: 0,
            last_frame: Instant::now(),
            simulation_accumulator: 0.0,
            time: 0.0,
            frame: 0,
            pending_clear: true,
            perf_overlay,
        }
    }
}

impl State {
    fn camera_basis(&self) -> (Vec3, Vec3, Vec3) {
        let basis = self.camera.basis();
        (basis.forward, basis.right, basis.right.cross(basis.forward))
    }

    fn update(&mut self) -> (bool, f32) {
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        self.time += dt;
        self.simulation_accumulator += dt;
        self.camera.update(self.input.take_input(), dt);
        if self.brush_active {
            self.brush_size = (self.brush_size - BRUSH_DRAIN_PER_SECOND * dt).max(BRUSH_SIZE_MIN);
        } else {
            self.brush_size =
                (self.brush_size + BRUSH_RECHARGE_PER_SECOND * dt).min(BRUSH_SIZE_MAX);
        }
        let run_simulation = self.pending_clear
            || self.brush_active
            || self.simulation_accumulator >= SIMULATION_INTERVAL;
        // Whether a paint stroke forces an update every display frame or the
        // idle simulation waits for its 30 Hz cadence, advance by the elapsed
        // simulation time since the last dispatch. Using only this frame's dt
        // in the idle path made the clouds appear slower whenever not painting.
        let simulation_delta = if run_simulation {
            self.simulation_accumulator.min(1.0 / 15.0)
        } else {
            0.0
        };
        if run_simulation {
            self.simulation_accumulator = 0.0;
        }
        self.write_uniforms(simulation_delta, self.pending_clear);
        self.pending_clear = false;
        self.frame = self.frame.wrapping_add(1);
        (run_simulation, dt)
    }

    fn write_uniforms(&self, delta: f32, clear: bool) {
        let perf_mode = PerfCycleMode::from_index(self.perf_mode_index);
        let art = self
            .art_preset
            .lock()
            .expect("art preset lock")
            .to_art_direction(self.time);
        let wind_cycle = (self.time * WIND_SPEED_RADIANS_PER_SECOND / std::f32::consts::TAU).rem_euclid(1.0);
        let wind_angle = WIND_INITIAL_ANGLE_RADIANS + wind_cycle * std::f32::consts::TAU;
        let wind = Vec3::new(
            wind_angle.sin() * WIND_STRENGTH,
            0.0,
            wind_angle.cos() * WIND_STRENGTH,
        );
        let sim: [f32; 28] = [
            delta,
            self.time,
            0.58,
            0.0,
            wind.x,
            wind.y,
            wind.z,
            0.84,
            self.brush_center.x,
            self.brush_center.y,
            self.brush_center.z,
            self.brush_size,
            0.38,
            self.brush_active as u8 as f32,
            self.brush_sign,
            2.0,
            0.15,
            1.41,
            -0.44,
            clear as u8 as f32,
            VOLUME.0 as f32,
            VOLUME.1 as f32,
            VOLUME.2 as f32,
            PRESET_SEED,
            1.0,
            art.curl,
            art.ribbon,
            art.sculpt,
        ];
        self.queue
            .write_buffer(&self.sim_uniform, 0, bytemuck::cast_slice(&sim));
        let (forward, right, up) = self.camera_basis();
        let cycle = (self.time / DAYLIGHT_CYCLE_SECONDS).fract();
        let daylight = if cycle <= 0.5 {
            cycle * 2.0
        } else {
            (1.0 - cycle) * 2.0
        };
        let sun_elevation = (SUN_ELEVATION_MIN_DEGREES
            + (SUN_ELEVATION_MAX_DEGREES - SUN_ELEVATION_MIN_DEGREES) * daylight)
            .to_radians();
        let exposure = EXPOSURE_MIN + (EXPOSURE_MAX - EXPOSURE_MIN) * daylight;
        let sun_azimuth = SUN_AZIMUTH_DEGREES.to_radians();
        let ce = sun_elevation.cos();
        let sun = Vec3::new(
            sun_azimuth.sin() * ce,
            sun_elevation.sin(),
            sun_azimuth.cos() * ce,
        )
        .normalize();
        let render: [f32; 68] = [
            self.config.width as f32,
            self.config.height as f32,
            self.time,
            self.frame as f32,
            self.camera.position().x,
            self.camera.position().y,
            self.camera.position().z,
            (PRESET_FOV_DEGREES.to_radians() * 0.5).tan(),
            forward.x,
            forward.y,
            forward.z,
            exposure,
            right.x,
            right.y,
            right.z,
            perf_mode.steps as f32,
            up.x,
            up.y,
            up.z,
            0.96,
            sun.x,
            sun.y,
            sun.z,
            1.31,
            1.0,
            0.871,
            0.651,
            1.45,
            0.07,
            0.16,
            0.42,
            0.27,
            0.74,
            0.57,
            0.43,
            PRESET_SEED,
            BOUNDS_RENDER_MIN.x,
            BOUNDS_RENDER_MIN.y,
            BOUNDS_RENDER_MIN.z,
            1.32,
            BOUNDS_RENDER_MAX.x,
            BOUNDS_RENDER_MAX.y,
            BOUNDS_RENDER_MAX.z,
            1.48,
            0.58,
            perf_mode.detail_tier as f32,
            0.0,
            0.0,
            1.0,
            art.bands,
            art.outline,
            art.sculpt,
            art.cloud.x,
            art.cloud.y,
            art.cloud.z,
            art.grain,
            art.shadow.x,
            art.shadow.y,
            art.shadow.z,
            art.ribbon,
            art.sky.x,
            art.sky.y,
            art.sky.z,
            art.moon_size,
            art.moon.x,
            art.moon.y,
            art.moon.z,
            art.moon_glow,
        ];
        self.queue
            .write_buffer(&self.render_uniform, 0, bytemuck::cast_slice(&render));

        let (forward, right, up) = self.camera_basis();
        let aspect = self.config.width.max(1) as f32 / self.config.height.max(1) as f32;
        let tan_half_fov = (PRESET_FOV_DEGREES.to_radians() * 0.5).tan();
        let outline: [f32; 24] = [
            self.camera.position().x,
            self.camera.position().y,
            self.camera.position().z,
            0.0,
            right.x,
            right.y,
            right.z,
            0.0,
            up.x,
            up.y,
            up.z,
            0.0,
            forward.x,
            forward.y,
            forward.z,
            0.0,
            1.0 / tan_half_fov,
            1.0 / aspect,
            CLOUDBOX_OUTLINE_EPS,
            0.0,
            CLOUDBOX_OUTLINE_COLOR[0],
            CLOUDBOX_OUTLINE_COLOR[1],
            CLOUDBOX_OUTLINE_COLOR[2],
            CLOUDBOX_OUTLINE_COLOR[3],
        ];
        self.queue
            .write_buffer(&self.outline_uniform, 0, bytemuck::cast_slice(&outline));
    }

    /// Converts a viewport cursor coordinate into the editable volume's
    /// normalized brush center, matching the original browser ray/AABB mapping.
    fn update_brush_from_cursor(&mut self, position: winit::dpi::PhysicalPosition<f64>) -> bool {
        let width = self.config.width.max(1) as f32;
        let height = self.config.height.max(1) as f32;
        let x = (position.x as f32 / width) * 2.0 - 1.0;
        let y = 1.0 - (position.y as f32 / height) * 2.0;
        let (forward, right, up) = self.camera_basis();
        let ray = (forward
            + right * x * (width / height) * (PRESET_FOV_DEGREES.to_radians() * 0.5).tan()
            + up * y * (PRESET_FOV_DEGREES.to_radians() * 0.5).tan())
        .normalize();
        let origin = self.camera.position();
        let inverse = ray.recip();
        let t0 = (BOUNDS_RENDER_MIN - origin) * inverse;
        let t1 = (BOUNDS_RENDER_MAX - origin) * inverse;
        let near = t0.min(t1).max_element();
        let far = t0.max(t1).min_element();
        if near > far || far < 0.0 {
            return false;
        }
        // Preset brush depth: 64% through the ray segment inside the volume.
        let world = origin + ray * (near.max(0.0) + (far - near.max(0.0)) * 0.64);
        self.brush_center = ((world - BOUNDS_SIM_MIN) / (BOUNDS_SIM_MAX - BOUNDS_SIM_MIN))
            .clamp(Vec3::splat(0.002), Vec3::splat(0.998));
        true
    }

    fn render(&mut self) {
        let (run_simulation, dt) = self.update();
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Cloud frame"),
            });
        let mut render_volume = self.current_volume;
        let view = output.texture.create_view(&Default::default());
        if run_simulation {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Cloud simulation"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &self.sim_groups[self.current_volume], &[]);
            pass.dispatch_workgroups(
                VOLUME.0.div_ceil(4),
                VOLUME.1.div_ceil(4),
                VOLUME.2.div_ceil(4),
            );
            render_volume = 1 - self.current_volume;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cloud raymarch"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &self.render_groups[render_volume], &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cloudbox outline"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.outline_pipeline);
            pass.set_bind_group(0, &self.outline_bind_group, &[]);
            pass.set_vertex_buffer(0, self.outline_vertex_buffer.slice(..));
            pass.draw(0..self.outline_vertex_count, 0..1);
        }

        self.perf_overlay
            .draw(
                dt,
                self.perf_mode_index,
                &mut encoder,
                &self.queue,
                &view,
                winit::dpi::PhysicalSize::new(self.config.width, self.config.height),
            );

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(output);
        self.current_volume = render_volume;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            self.state = Some(Self::create_state(event_loop, self.art_preset.clone()));
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => state.render(),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                state.config.width = size.width;
                state.config.height = size.height;
                state.surface.configure(&state.device, &state.config);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !state.input.cursor_grabbed() {
                    if state
                        .pointer_position
                        .is_some_and(|position| PerfOverlay::is_click_within(position, state.config.width, state.config.height))
                    {
                        state.perf_mode_index = (state.perf_mode_index + 1) % PERF_MODES.len() as u32;
                        state.brush_active = false;
                        state
                            .perf_overlay
                            .update_text(&state.queue, state.perf_overlay.last_fps, state.perf_mode_index);
                    } else {
                        state.brush_sign = 1.0;
                        state.brush_active = state
                            .pointer_position
                            .is_some_and(|position| state.update_brush_from_cursor(position));
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } if !state.input.cursor_grabbed() => {
                state.brush_sign = -1.0;
                state.brush_active = state
                    .pointer_position
                    .is_some_and(|position| state.update_brush_from_cursor(position));
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left | MouseButton::Right,
                ..
            } if !state.input.cursor_grabbed() => state.brush_active = false,
            WindowEvent::CursorMoved { position, .. } if !state.input.cursor_grabbed() => {
                state.pointer_position = Some(position);
                let valid = state.update_brush_from_cursor(position);
                state.brush_active &= valid;
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state: key_state,
                        repeat,
                        ..
                    },
                ..
            } => {
                if key_state == ElementState::Pressed && !repeat && key == KeyCode::KeyF {
                    state.brush_active = false;
                    if state.input.cursor_grabbed() {
                        state.input.release_cursor(&state.window);
                    } else {
                        state.input.grab_cursor(&state.window);
                    }
                } else if key_state == ElementState::Pressed && !repeat && key == KeyCode::Escape {
                    if !state.input.release_cursor(&state.window) {
                        event_loop.exit();
                    }
                } else if state.input.cursor_grabbed() {
                    state.input.set_key(key, key_state == ElementState::Pressed);
                }
            }
            WindowEvent::Focused(focused) => {
                state.input.set_window_focused(&state.window, focused);
                if !focused {
                    state.brush_active = false;
                }
            }
            _ => {}
        }
    }
    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if let (Some(state), DeviceEvent::MouseMotion { delta }) = (self.state.as_mut(), event) {
            state.input.add_mouse_motion(delta.0, delta.1);
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
