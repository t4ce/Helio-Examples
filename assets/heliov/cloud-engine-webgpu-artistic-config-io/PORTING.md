# Porting Cloud Engine to SPIR-V, Intel IGC, and C++

## The important toolchain distinction

Intel Graphics Compiler (IGC) is downstream of SPIR-V in the normal Intel compute stack. In other words, the practical pipeline is not “IGC emits SPIR-V.” A shader-language frontend produces SPIR-V; the Intel runtime/IGC consumes that intermediate form and compiles it for the selected Intel GPU.

For this project, the least-friction native path is:

```text
OpenCL C twin kernels
        ↓ OpenCL Clang / LLVM
LLVM bitcode
        ↓ SPIRV-LLVM-Translator
SPIR-V compute modules
        ↓ Level Zero or OpenCL runtime
Intel IGC
        ↓
Intel GPU machine code
```

The IGC source tree itself depends on OpenCL Clang and SPIRV-LLVM-Translator, which is why the native twins are supplied in OpenCL C rather than relying on a fragile source-to-source conversion from WGSL.

A second viable path is WGSL → Tint/Naga → SPIR-V → runtime/IGC. That reduces duplicated shader source, but it makes generated SPIR-V and feature mapping dependent on the translator version. For a product codebase, choose one canonical source language and enforce numerical regression tests between backends.

## Why this prototype ports cleanly

The simulation kernel was written around the common denominator of WebGPU compute and native GPU compute:

- fixed workgroup dimensions;
- bounded loops;
- no derivatives;
- no subgroups;
- no atomics;
- no storage-buffer pointer graphs;
- no dynamically indexed resource arrays;
- no browser callbacks inside shader logic;
- explicit ping-pong images;
- 16-byte-aligned `float4` parameter blocks.

The web renderer is a fragment shader only because a WebGPU canvas is a render attachment. The native version in `native/cloud_render.cl` is already expressed as a compute kernel with one work-item per pixel.

## Resource contract

### Simulation dispatch

| Item | Value |
| --- | --- |
| Volume format | four 16-bit floats per voxel (`rgba16float` / equivalent native image format) |
| Volume extent | `96 × 48 × 96` |
| Workgroup | `4 × 4 × 4` |
| Workgroups | `24 × 12 × 24` |
| Input | read-only sampled 3D image |
| Output | write-only storage 3D image |
| Sampler | normalized coordinates, linear filtering, repeat X/Z, clamp Y |
| Parameters | `SimParams`, 112 bytes |

OpenCL samplers expose one addressing mode for all axes. The native kernel therefore wraps X/Z numerically and clamps Y before sampling; a repeat sampler is safe after that adjustment.

### Native render dispatch

| Item | Value |
| --- | --- |
| Workgroup | `8 × 8 × 1` |
| Workgroups | `ceil(width/8) × ceil(height/8) × 1` |
| Input | current read-only 3D cloud volume |
| Output | write-only 2D display image |
| Parameters | `RenderParams`, 272 bytes |

The output can be an `RGBA8 UNORM`, `RGBA16F`, or swapchain-compatible intermediate. The kernel currently writes tone-mapped display values, so `RGBA8 UNORM` is sufficient. Keep an HDR output instead when post-processing will happen later.

## Binding correspondence

### WebGPU simulation

```text
binding 0  uniform SimParams
binding 1  previous sampled 3D volume
binding 2  linear sampler
binding 3  next write-only 3D volume
```

### OpenCL simulation kernel arguments

```c
constant SimParams* params,
read_only image3d_t previous_volume,
sampler_t volume_sampler,
write_only image3d_t next_volume
```

### WebGPU render

```text
binding 0  uniform RenderParams
binding 1  sampled current 3D volume
binding 2  linear sampler
```

### OpenCL render kernel arguments

```c
constant RenderParams* params,
read_only image3d_t cloud_volume,
sampler_t volume_sampler,
write_only image2d_t output_image
```

## ABI rules

Use `native/cloud_params.hpp` as the C++ contract.

- Do not replace a `Float4` with a native compiler vector type unless its ABI is proven for every target compiler.
- Upload the whole structure as bytes.
- Keep all fields as 32-bit floats, including pattern IDs, booleans, quality tiers, and volume dimensions, because that matches the WGSL blocks.
- Encode booleans as `0.0f` or `1.0f`.
- Encode the pattern ID as an integer-valued float.
- Preserve 16-byte structure alignment.
- `SimParams::art_controls` carries artistic-mode enable, curl, ribbon stretch, and sculpt controls.
- `RenderParams` appends the artistic style, palette, grain, moon-size, and moon-glow vectors after the original render fields.

The header statically verifies:

```text
sizeof(SimParams)    == 112
sizeof(RenderParams) == 272
```

## Representative SPIR-V build

Exact executable names and supported LLVM versions depend on the Intel graphics stack revision. A common two-stage flow looks like this:

```bash
# Representative commands; use the OpenCL Clang and translator versions paired
# with the target IGC checkout/runtime.
opencl-clang -x cl -cl-std=CL2.0 -emit-llvm -c \
  native/cloud_simulation.cl -o cloud_simulation.bc
llvm-spirv cloud_simulation.bc -o cloud_simulation.spv

opencl-clang -x cl -cl-std=CL2.0 -emit-llvm -c \
  native/cloud_render.cl -o cloud_render.bc
llvm-spirv cloud_render.bc -o cloud_render.spv

spirv-val cloud_simulation.spv
spirv-val cloud_render.spv
```

Some toolchains can target SPIR-V directly. Prefer the flow documented by the exact OpenCL Clang/SPIRV-LLVM-Translator/IGC release combination rather than mixing arbitrary LLVM majors.

## C++ frame sequence

A native frame has the same ordering as `app.js`:

```cpp
// 1. Update SimParams and RenderParams upload buffers.
// 2. Optionally dispatch simulation into the opposite volume.
// 3. Insert an image/resource barrier: simulation write -> render sample.
// 4. Dispatch cloud_render over the output image.
// 5. Copy/transition the output image for presentation or encoding.
// 6. Swap currentVolume = 1 - currentVolume when simulation ran.
```

When paused, skip simulation unless the brush is active or a clear pulse is pending. A brush update uses `dt = 0`, so it edits density without advancing advection.

## Level Zero host outline

A Level Zero implementation needs:

1. driver/device/context selection;
2. a command queue and reusable command list(s);
3. two 3D images with sampled + storage capability;
4. one output image;
5. parameter upload allocations;
6. modules created from the two SPIR-V binaries;
7. kernels named `cloud_simulation` and `cloud_render`;
8. argument binding matching the tables above;
9. image barriers between compute stages;
10. presentation/encode integration chosen by the application.

Keep browser UI/networking out of the native engine layer. Map UI values into the same POD structures from a separate control service, desktop UI, or RPC layer.

## OpenCL host outline

An OpenCL implementation follows the same resource graph:

- create two `CL_MEM_OBJECT_IMAGE3D` images;
- create one normalized linear sampler;
- create `cl_mem` buffers for the parameter blocks;
- set kernel arguments in the listed order;
- enqueue `cloud_simulation` with global size rounded up to `96 × 48 × 96` and local size `4 × 4 × 4`;
- enqueue `cloud_render` with rounded-up output dimensions and local size `8 × 8`;
- use events or in-order queues to preserve write/read ordering.

## JSON configuration handoff

The browser exports a versioned `cloud-engine-config` JSON document. This is the bridge from the prototype UI into a native C++ host. SPIR-V kernels do not parse JSON themselves: the host loads the document, validates and converts its values, fills `SimParams` and `RenderParams`, and uploads those POD blocks exactly as it would for values coming from a desktop UI or RPC service.

The top-level configuration groups follow the panel:

```text
configuration.mode
configuration.formation
configuration.look
configuration.air
configuration.brush
configuration.camera
configuration.sunAndRender
configuration.playback
```

Important conversion rules:

- `formation.pattern` is uploaded as an integer-valued float in `SimParams::brush_controls.w`.
- `formation.amount`, mode, wind, turbulence, brush values, dissipation, tearing, rotation, seed, and artistic flow controls feed `SimParams` in the same order used by `updateSimulationUniforms()` in `app.js`.
- camera pose, sun values, exposure, quality, palette values, moon controls, grain, and artistic lighting controls feed `RenderParams` in the same order used by `updateRenderUniforms()`.
- `sunColor` and all artistic colors are sRGB `#RRGGBB` strings. Convert them to linear RGB before placing them in `RenderParams`; `app.js::hexToLinearRgb()` is the reference conversion.
- wind direction, sun azimuth/elevation, and camera field of view are stored in degrees. Camera yaw and pitch are stored in radians.
- `quality` is a host-side enum. It selects render-step count, detail level, quality tier, and output pixel-ratio policy before the resulting numeric values are written to `RenderParams`.
- `playback.paused` controls dispatch scheduling. It is not a simulation ABI field, although the browser also places a paused indicator in the render block.

The JSON stores the procedural seed and camera pose but not the mutable 3D density texture. Loading it in the browser resets simulation time and restarts the volume from the saved seed. A native host can use the same rule for deterministic preset loading, or preserve its current volume when it only wants to apply parameters live.

The format/version pair is currently:

```json
{
  "format": "cloud-engine-config",
  "version": 1
}
```

Reject unsupported versions rather than guessing field meanings. Unknown extra keys can be ignored so later exporters can remain backward compatible.

## Keeping WGSL and native kernels aligned

Treat these pairs as linked files:

```text
shaders/simulate.wgsl  <-> native/cloud_simulation.cl
shaders/render.wgsl    <-> native/cloud_render.cl
```

For every shader change:

1. update the twin implementation;
2. run the browser smoke test;
3. run Clang OpenCL syntax checks;
4. compile the C++ ABI check;
5. compare deterministic frames with fixed seed/time/parameters;
6. allow a small per-pixel tolerance because math-library and filtering implementations differ.

A useful regression test stores an `RGBA16F` or float output from each backend and compares luminance RMSE plus a structural image metric. Do not require bit identity across shader compilers.

## Production extensions that preserve the contract

- Increase the volume dimensions while keeping ping-pong behavior.
- Add a lower-resolution lighting cache as another compute image.
- Add temporal accumulation in the native render output.
- Replace analytic air motion with a pressure-projected velocity texture.
- Add weather-map and height-profile inputs as extra sampled images.
- Stream UI changes over WebSocket or RPC into the same C++ parameter blocks.
- Run headless and encode the compute output through a hardware video path.

The resource graph can grow without changing the core simulation/render separation.

## References

- WebGPU specification: `https://gpuweb.github.io/gpuweb/`
- WGSL specification: `https://gpuweb.github.io/gpuweb/wgsl.html`
- Intel Graphics Compiler: `https://github.com/intel/intel-graphics-compiler`
- IGC shader-dump/toolchain notes: `https://github.com/intel/intel-graphics-compiler/blob/master/documentation/shader_dumps.md`
