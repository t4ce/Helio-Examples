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
const BOUNDS_MIN: Vec3 = Vec3::new(-8.0, -2.0, -1.0);
const BOUNDS_MAX: Vec3 = Vec3::new(8.0, 9.0, 18.0);
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

const SIM_SHADER: &str = include_str!("cloud-engine-webgpu-linux-aligned/shaders/simulate.wgsl");
const RENDER_SHADER: &str = include_str!("cloud-engine-webgpu-linux-aligned/shaders/render.wgsl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtPreset {
    Verdant,
    Porcelain,
    Ember,
    Violet,
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

    let start_preset = preset_override.unwrap_or(ArtPreset::Porcelain);

    EventLoop::new()
        .expect("event loop")
        .run_app(&mut App::new(start_preset))
        .expect("event loop error");
}

struct App {
    state: Option<State>,
    art_preset: Arc<Mutex<ArtPreset>>,
}

impl App {
    fn new(start_preset: ArtPreset) -> Self {
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
    sim_uniform: wgpu::Buffer,
    render_uniform: wgpu::Buffer,
    sim_groups: [wgpu::BindGroup; 2],
    render_groups: [wgpu::BindGroup; 2],
    current_volume: usize,
    camera: FlyCamera,
    input: WinitFlyInput,
    pointer_position: Option<winit::dpi::PhysicalPosition<f64>>,
    brush_center: Vec3,
    brush_active: bool,
    brush_sign: f32,
    brush_size: f32,
    art_preset: Arc<Mutex<ArtPreset>>,
    last_frame: Instant,
    simulation_accumulator: f32,
    time: f32,
    frame: u32,
    pending_clear: bool,
}

impl App {
    fn create_state(event_loop: &ActiveEventLoop, art_preset: Arc<Mutex<ArtPreset>>) -> State {
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
        State {
            window,
            surface,
            device,
            queue,
            config,
            compute_pipeline,
            render_pipeline,
            sim_uniform,
            render_uniform,
            sim_groups,
            render_groups,
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
            last_frame: Instant::now(),
            simulation_accumulator: 0.0,
            time: 0.0,
            frame: 0,
            pending_clear: true,
        }
    }
}

impl State {
    fn camera_basis(&self) -> (Vec3, Vec3, Vec3) {
        let basis = self.camera.basis();
        (basis.forward, basis.right, basis.right.cross(basis.forward))
    }

    fn update(&mut self) -> bool {
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
        run_simulation
    }

    fn write_uniforms(&self, delta: f32, clear: bool) {
        let art = self.art_preset.lock().expect("art preset lock").direction();
        let wind_radians = 5.0_f32.to_radians();
        let wind = Vec3::new(wind_radians.sin() * 1.95, 0.0, wind_radians.cos() * 1.95);
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
            70.0,
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
            BOUNDS_MIN.x,
            BOUNDS_MIN.y,
            BOUNDS_MIN.z,
            1.32,
            BOUNDS_MAX.x,
            BOUNDS_MAX.y,
            BOUNDS_MAX.z,
            1.48,
            0.58,
            2.0,
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
        let t0 = (BOUNDS_MIN - origin) * inverse;
        let t1 = (BOUNDS_MAX - origin) * inverse;
        let near = t0.min(t1).max_element();
        let far = t0.max(t1).min_element();
        if near > far || far < 0.0 {
            return false;
        }
        // Preset brush depth: 64% through the ray segment inside the volume.
        let world = origin + ray * (near.max(0.0) + (far - near.max(0.0)) * 0.64);
        self.brush_center = ((world - BOUNDS_MIN) / (BOUNDS_MAX - BOUNDS_MIN))
            .clamp(Vec3::splat(0.002), Vec3::splat(0.998));
        true
    }

    fn render(&mut self) {
        let run_simulation = self.update();
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
            let view = output.texture.create_view(&Default::default());
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
                    state.brush_sign = 1.0;
                    state.brush_active = state
                        .pointer_position
                        .is_some_and(|position| state.update_brush_from_cursor(position));
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
