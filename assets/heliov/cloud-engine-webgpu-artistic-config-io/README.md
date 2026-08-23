# Cloud Engine — WebGPU Artistic + Volumetric + Config I/O Edition

A focused cloud renderer and editable 3D density simulation for the browser. It fills the screen with clouds, exposes a compact control panel, supports direct mouse painting, and keeps its shader/resource contract close to native compute so it can later be carried to SPIR-V, C++, OpenCL, Level Zero, and Intel IGC.

This consolidated package contains the complete application. No manual file replacement is required.

![Cloud Engine preview](assets/cloud-engine-preview.png)

## Run

WebGPU shader modules are fetched over HTTP, so do not open `index.html` through `file://`.

```bash
cd cloud-engine-webgpu-artistic-config-io
python3 serve.py --open
```

Then open:

```text
http://localhost:8080/
```

There is no npm install, package manager, build step, bundler, framework, or CDN dependency.

## Main features

- Fullscreen WebGPU cloud renderer with no scene geometry.
- Ping-pong 3D `rgba16float` density simulation.
- Seven procedural formations:
  - Cumulus field
  - Layered deck
  - High wisps
  - Storm towers
  - Broken cells
  - Moon scrolls
  - Wind ribbons
- Continuous Auto mode with cloud-amount control.
- Draw mode for adding and erasing cloud density directly in the scene.
- Correct full-canvas pointer-to-volume mapping, including the lower half of the frame.
- Painting while paused.
- World-space spherical brush with size, strength, and depth controls.
- Wind translation, turbulence, soft tearing, drift rotation, fading, and stylized curl/ribbon flow.
- Realistic volumetric rendering and a separate artistic moon-scroll renderer.
- Collapsible responsive HTML/CSS control panel.
- Free-fly camera with pointer-lock mouse look and WASD movement.
- Sun color, direction, intensity, exposure, and quality controls.
- PNG capture.
- Versioned JSON export/import for every panel value, the camera pose, pause state, and procedural seed.
- OpenCL C compute twins and matching C++ parameter layouts under `native/`.

## Artistic mode

Select **Artistic moon-scroll** under **Look**. It adds an art-directed rendering path inspired by sculpted, curling painted clouds rather than applying a simple color grade to the realistic result.

Controls include:

- Verdant moon, blue porcelain, amber dusk, violet night, and custom palettes.
- Independent cloud, shadow, sky, and moon colors.
- Curl-field strength.
- Ribbon stretch.
- Sculpted-body response.
- Quantized toon-lighting bands.
- Ink-like cloud edges.
- Moon angular size and glow.
- Print grain.

The **Moon scrolls** and **Wind ribbons** formations are designed for this mode, but every formation can be rendered with either style.

## Interaction

| Input | Action |
| --- | --- |
| `Space` | Pause/resume cloud simulation |
| `H` | Collapse/expand control panel |
| `C` | Clear density volume |
| `F` | Enter or leave flycam |
| Double-click canvas in Auto mode | Enter flycam |
| Mouse in flycam | Look around |
| `W A S D` | Fly forward, left, backward, right |
| `Q / E` | Fly down/up |
| `Shift` in flycam | Speed boost |
| `Esc` | Release pointer lock |
| Left-drag in Draw mode | Add cloud density |
| `Shift` + drag or right-drag | Erase cloud density |
| Mouse wheel in Draw mode | Change brush depth |

The camera can move while the simulation is paused. Leaving flycam returns the canvas to drawing interaction when Draw mode is active.

## JSON configuration export and load

The **Configuration** section at the bottom of the panel has two controls:

- **Export JSON** downloads one readable, versioned configuration file.
- **Load JSON** validates that file, clamps numeric values to the current UI ranges, applies every setting, restores the camera pose and pause state, and restarts the volume from the saved procedural seed.

The document identifies itself with:

```json
{
  "format": "cloud-engine-config",
  "version": 1,
  "engineVersion": "2.1-config-io",
  "configuration": {
    "mode": "auto",
    "formation": {},
    "look": {},
    "air": {},
    "brush": {},
    "camera": {},
    "sunAndRender": {},
    "playback": {}
  }
}
```

The exported groups contain every current panel control:

- formation mode, pattern, amount, and seed;
- realistic/artistic selection, palette colors, curl, ribbon, sculpt, bands, outline, moon, and grain controls;
- all air, brush, camera, sun, exposure, quality, and pause values;
- the live fly-camera position, yaw, and pitch, even though those are moved with the mouse and keyboard rather than sliders.

Color values are stored as sRGB `#RRGGBB` strings. Wind direction, sun angles, and field of view are degrees. Camera yaw and pitch are radians. The remaining scalar values use the same units and ranges as the panel and native parameter preparation in `app.js`.

The JSON is deliberately a **configuration handoff**, not a 3D texture dump. The mutable `96 × 48 × 96` density volume and hand-painted voxel contents are not embedded. Loading therefore starts a clean procedural volume in Auto mode and an empty volume in Draw mode.

## Architecture

### Compute simulation

`shaders/simulate.wgsl` dispatches one invocation per voxel over a `96 × 48 × 96` volume. Two textures alternate roles:

```text
volume A --sample--> simulation kernel --write--> volume B
volume B --sample--> simulation kernel --write--> volume A
```

Each voxel stores advected density and supporting procedural/noise channels. The simulation uses fixed `4 × 4 × 4` workgroups and ordinary sampled/storage textures. It avoids atomics, subgroups, derivatives, dynamic resource arrays, and shader-language-specific pointer structures.

### Browser renderer

`shaders/render.wgsl` emits a fullscreen triangle from `vertex_index`; there are no mesh or vertex-buffer dependencies. The fragment stage ray marches the cloud AABB.

The realistic path includes extinction, short sun-light marches, anisotropic phase response, powder/silver-lining behavior, ambient contribution, tone mapping, and dithering.

The artistic path adds a procedural moon, sculpted density response, lighting quantization, outlines, palette control, and grain while sampling the same editable 3D volume.

### Native renderer

`native/cloud_render.cl` expresses the render stage as an `8 × 8` compute kernel that writes one output pixel per work-item. This is the intended native/C++ shape and does not require a graphics pipeline.

## Portability contract

| Stage | WGSL | Native compute | C++ POD |
| --- | --- | --- | --- |
| Simulation | `shaders/simulate.wgsl` | `native/cloud_simulation.cl` | `native/cloud_params.hpp::SimParams` |
| Render | `shaders/render.wgsl` | `native/cloud_render.cl` | `native/cloud_params.hpp::RenderParams` |

Current ABI sizes:

```text
SimParams:    112 bytes
RenderParams: 272 bytes
```

`native/abi_check.cpp` contains compile-time assertions and a runtime printout. See [`PORTING.md`](PORTING.md) and [`native/README.md`](native/README.md) for the SPIR-V, OpenCL, Level Zero, and Intel IGC handoff.

## Validation

Run the static checks with:

```bash
./verify.sh
```

They cover:

- JavaScript syntax.
- Python helper syntax.
- OpenCL C 2.0 syntax for both native kernels.
- C++20 ABI assertions and structure sizes.

The optional browser smoke test is:

```bash
python3 tests/smoke_test.py
```

A configuration-only browser test that does not need localhost or WebGPU is also included:

```bash
python3 tests/config_io_test.py
```

The full smoke test requires Playwright/Chromium and an environment that permits browser access to localhost; it now includes the JSON round-trip after its rendering and interaction checks. The configuration-only test also requires Playwright/Chromium, but it does not need localhost or WebGPU and still performs an actual download, edits every major configuration group, loads the file through the UI, and verifies the result.

## Source map

```text
index.html                  interface and overlays
styles.css                  responsive dependency-free UI
app.js                      WebGPU host, flycam, drawing, controls, and JSON configuration I/O
shaders/simulate.wgsl       3D cloud compute simulation
shaders/render.wgsl         realistic + artistic fullscreen renderer
serve.py                    no-dependency localhost server
verify.sh                   static validation entry point
PORTING.md                  native/SPIR-V/IGC architecture notes
native/cloud_params.hpp     matching C++ parameter ABI
native/cloud_simulation.cl  native simulation compute twin
native/cloud_render.cl      native render compute twin
native/abi_check.cpp        layout verification executable
native/CMakeLists.txt       ABI-check CMake target
tests/smoke_test.py         optional full WebGPU browser interaction test
tests/config_io_test.py     browser JSON round-trip test without a local server
assets/                     preview media
```

## Scope

This is an engine nucleus rather than a pressure-projected meteorological solver. The air field is analytic and art-directable; the lighting is physically inspired rather than a spectral atmospheric reference integrator. The implementation intentionally prioritizes visual quality, controllability, direct editing, and a clean native-compute migration path.
