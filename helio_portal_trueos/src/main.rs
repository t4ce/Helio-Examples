#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use helio_portal_trueos::portal_rooms;
use trueos::input::{KEYBOARD_KEY_TAB, KEYBOARD_OUTPUT_FLAG_PRESS, KEYBOARD_OUTPUT_KIND_KEY};
use trueos::ui4_scene::{Damage, Frame, POINTER_BUTTON_PRIMARY, POINTER_BUTTON_SECONDARY};
use trueos::vgpu::{
    BUFFER_USAGE_INDEX, BUFFER_USAGE_MAP_WRITE, BUFFER_USAGE_VERTEX, Buffer, Capabilities, Device,
    IndexedBatchDrawV2, IndexedDrawBatchV2, MAX_INDEXED_BATCH_V2_DRAWS,
    PRIMITIVE_TOPOLOGY_TRIANGLE_LIST, Queue, QueueClass, RenderPipeline,
    SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64,
};
use trueos::{clock, logl, vsys};
use trueos_helio_runtime::Camera;

use helio_portal_trueos::portal_rooms::Batch;

const SCENE_ARTIFACT: &[u8] =
    include_bytes!("../../../TRUEOS/picasso/simple-cube.trueos.intel.helio");
const FRAME_X: i32 = 188;
const FRAME_Y: i32 = 124;
const INITIAL_WIDTH: u32 = 960;
const INITIAL_HEIGHT: u32 = 600;
const FRAME_MS: u64 = 16;
const CLEAR: u32 = u32::from_le_bytes([0, 0, 0, 255]);

const HID_A: u8 = 0x04;
const HID_D: u8 = 0x07;
const HID_S: u8 = 0x16;
const HID_W: u8 = 0x1a;
const HID_SPACE: u8 = 0x2c;
const HID_CONTROL_LEFT: u8 = 0xe0;
const HID_SHIFT_LEFT: u8 = 0xe1;
const HID_CONTROL_RIGHT: u8 = 0xe4;
const HID_SHIFT_RIGHT: u8 = 0xe5;

fn main() {
    if let Err(stage) = run() {
        logl::log(
            logl::level::ERROR,
            format_args!("helio_portal_trueos: stopped at {stage}"),
        );
    }
}

fn run() -> Result<(), &'static str> {
    let spec = portal_rooms::Spec::decode_artifact(SCENE_ARTIFACT).map_err(|_| "scene decode")?;
    let mut engine = portal_rooms::Engine::new(spec).map_err(|_| "scene create")?;
    let mut camera = FlyCamera::new(engine.camera());
    let mut frame = Frame::open_streaming(FRAME_X, FRAME_Y, INITIAL_WIDTH, INITIAL_HEIGHT)
        .map_err(|_| "frame open")?;
    let renderer = Renderer::new(engine.batches()).map_err(|_| "GPU setup")?;
    let mut width = INITIAL_WIDTH;
    let mut height = INITIAL_HEIGHT;
    let mut previous_ms = clock::monotonic_millis();

    logl::log(
        logl::level::INFO,
        format_args!(
            "helio_portal_trueos: {} ready; rooms={} objects={}; controls={}",
            engine.name(),
            engine.portal_count(),
            engine.object_count(),
            engine.controls(),
        ),
    );

    loop {
        vsys::poll_once();
        drain_resize(&mut frame, &mut width, &mut height)?;
        drain_actions(&mut frame, &mut engine)?;

        let now_ms = clock::monotonic_millis();
        let dt = now_ms.saturating_sub(previous_ms).clamp(1, 50) as f32 / 1_000.0;
        previous_ms = now_ms;
        camera.update(&mut frame, dt)?;
        engine
            .set_camera(camera.camera())
            .map_err(|_| "camera update")?;

        let batches = engine
            .step(width as f32 / height.max(1) as f32)
            .map_err(|_| "scene step")?;
        renderer.present(&mut frame, batches, CLEAR)?;
        vsys::sleep_ms(FRAME_MS);
    }
}

fn drain_resize(frame: &mut Frame, width: &mut u32, height: &mut u32) -> Result<(), &'static str> {
    while let Some(event) = frame.take_resize_event().map_err(|_| "resize event")? {
        if event.width == 0 || event.height == 0 {
            continue;
        }
        frame
            .resize(event.width, event.height)
            .map_err(|_| "resize")?;
        *width = event.width;
        *height = event.height;
    }
    Ok(())
}

fn drain_actions(frame: &mut Frame, engine: &mut portal_rooms::Engine) -> Result<(), &'static str> {
    while let Some(event) = frame.take_keyboard_event().map_err(|_| "keyboard event")? {
        if event.flags & KEYBOARD_OUTPUT_FLAG_PRESS != 0
            && event.kind == KEYBOARD_OUTPUT_KIND_KEY
            && event.key_code == KEYBOARD_KEY_TAB
        {
            engine.toggle_editor_mode();
            logl::log(
                logl::level::INFO,
                format_args!(
                    "helio_portal_trueos: portal_overlay={}",
                    engine.editor_mode(),
                ),
            );
        }
    }
    Ok(())
}

struct FlyCamera {
    camera: Camera,
    yaw: f32,
    pitch: f32,
}

impl FlyCamera {
    fn new(camera: Camera) -> Self {
        let forward = normalize(sub(camera.target, camera.position));
        Self {
            camera,
            yaw: libm::atan2f(forward[2], forward[0]),
            pitch: libm::asinf(forward[1].clamp(-1.0, 1.0)),
        }
    }

    fn camera(&self) -> Camera {
        self.camera
    }

    fn update(&mut self, frame: &mut Frame, dt: f32) -> Result<(), &'static str> {
        while let Some(event) = frame.take_pointer_event().map_err(|_| "pointer event")? {
            if event.buttons_down & POINTER_BUTTON_PRIMARY != 0
                && event.buttons_down & POINTER_BUTTON_SECONDARY == 0
            {
                self.yaw += event.dx as f32 * 0.003;
                self.pitch = (self.pitch - event.dy as f32 * 0.003).clamp(-1.52, 1.52);
            }
        }

        let forward = [
            libm::cosf(self.pitch) * libm::cosf(self.yaw),
            libm::sinf(self.pitch),
            libm::cosf(self.pitch) * libm::sinf(self.yaw),
        ];
        let right = normalize([-forward[2], 0.0, forward[0]]);
        let mut motion = [0.0; 3];
        if let Some(keys) = frame.keyboard_state().map_err(|_| "keyboard state")? {
            if keys.is_down(HID_W) {
                motion = add(motion, forward);
            }
            if keys.is_down(HID_S) {
                motion = sub(motion, forward);
            }
            if keys.is_down(HID_D) {
                motion = add(motion, right);
            }
            if keys.is_down(HID_A) {
                motion = sub(motion, right);
            }
            if keys.is_down(HID_SPACE) {
                motion[1] += 1.0;
            }
            if keys.is_down(HID_SHIFT_LEFT) || keys.is_down(HID_SHIFT_RIGHT) {
                motion[1] -= 1.0;
            }
            let boost = if keys.is_down(HID_CONTROL_LEFT) || keys.is_down(HID_CONTROL_RIGHT) {
                3.0
            } else {
                1.0
            };
            let distance = 10.0 * boost * dt;
            let motion = normalize_or_zero(motion);
            self.camera.position = add(self.camera.position, scale(motion, distance));
        }
        self.camera.target = add(self.camera.position, forward);
        Ok(())
    }
}

struct Renderer {
    device: Device,
    queue: Queue,
    pipeline: RenderPipeline,
    vertices: Buffer,
    indices: Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
}

impl Renderer {
    fn new(batches: &[Batch]) -> Result<Self, i32> {
        if batches.is_empty() || batches.len() > MAX_INDEXED_BATCH_V2_DRAWS {
            return Err(-22);
        }
        let vertex_capacity = batches
            .iter()
            .try_fold(0usize, |sum, batch| {
                sum.checked_add(batch.vertices.len() * 12)
            })
            .ok_or(-12)?;
        let index_capacity = batches
            .iter()
            .try_fold(0usize, |sum, batch| {
                sum.checked_add(batch.indices.len() * 4)
            })
            .ok_or(-12)?;
        let device = Device::open(Capabilities::DEFAULT.union(Capabilities::PRESENT))?;
        let queue = device.create_queue(QueueClass::Render)?;
        let shader =
            device.create_shader_module(SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64)?;
        let pipeline = device.create_render_pipeline(shader, 12, 0)?;
        device.destroy_shader_module(shader)?;
        let vertices = device.create_buffer(
            vertex_capacity,
            BUFFER_USAGE_VERTEX | BUFFER_USAGE_MAP_WRITE,
        )?;
        let indices =
            device.create_buffer(index_capacity, BUFFER_USAGE_INDEX | BUFFER_USAGE_MAP_WRITE)?;
        Ok(Self {
            device,
            queue,
            pipeline,
            vertices,
            indices,
            vertex_capacity,
            index_capacity,
        })
    }

    fn present(
        &self,
        frame: &mut Frame,
        batches: &[Batch],
        clear: u32,
    ) -> Result<(), &'static str> {
        let mut vertex_bytes = Vec::new();
        let mut index_bytes = Vec::new();
        vertex_bytes.reserve(self.vertex_capacity);
        index_bytes.reserve(self.index_capacity);
        let mut submit = IndexedDrawBatchV2 {
            clear_rgba8_srgb: clear,
            ..IndexedDrawBatchV2::default()
        };
        let mut vertex_count = 0usize;
        let mut index_count = 0usize;
        let mut draw_count = 0usize;
        for source in batches {
            if source.indices.is_empty() {
                continue;
            }
            if draw_count >= MAX_INDEXED_BATCH_V2_DRAWS {
                return Err("draw capacity");
            }
            let base_vertex = i32::try_from(vertex_count).map_err(|_| "vertex range")?;
            let first_index = u32::try_from(index_count).map_err(|_| "index range")?;
            for point in &source.vertices {
                for component in point {
                    vertex_bytes.extend_from_slice(&component.to_le_bytes());
                }
            }
            for index in &source.indices {
                index_bytes.extend_from_slice(&index.to_le_bytes());
            }
            submit.draws[draw_count] = IndexedBatchDrawV2 {
                index_count: u32::try_from(source.indices.len()).map_err(|_| "index count")?,
                first_index,
                base_vertex,
                rgba8_srgb: u32::from_le_bytes(source.rgba),
                topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
                reserved: 0,
            };
            vertex_count += source.vertices.len();
            index_count += source.indices.len();
            draw_count += 1;
        }
        if vertex_bytes.len() > self.vertex_capacity || index_bytes.len() > self.index_capacity {
            return Err("GPU buffer capacity");
        }
        submit.draw_count = u32::try_from(draw_count).map_err(|_| "draw count")?;
        self.device
            .write_buffer(self.vertices, 0, &vertex_bytes)
            .map_err(|_| "vertex upload")?;
        self.device
            .write_buffer(self.indices, 0, &index_bytes)
            .map_err(|_| "index upload")?;
        frame.begin_gpu_frame().map_err(|_| "frame begin")?;
        let surface = self
            .device
            .acquire_ui4_surface(frame.window_id())
            .map_err(|_| "surface acquire")?;
        self.device
            .submit_ui4_indexed_batch_v2(
                self.queue,
                surface,
                self.pipeline,
                self.vertices,
                self.indices,
                submit,
            )
            .map_err(|_| "GPU submit")?;
        frame
            .publish(Damage::full(frame.width(), frame.height()))
            .map_err(|_| "frame publish")
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = libm::sqrtf(value[0] * value[0] + value[1] * value[1] + value[2] * value[2]);
    scale(value, 1.0 / length.max(0.000_001))
}

fn normalize_or_zero(value: [f32; 3]) -> [f32; 3] {
    let length = libm::sqrtf(value[0] * value[0] + value[1] * value[1] + value[2] * value[2]);
    if length > 0.000_001 {
        scale(value, 1.0 / length)
    } else {
        [0.0; 3]
    }
}
