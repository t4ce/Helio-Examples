# Cloud Engine native alignment: Linux authoring -> TRUEOS Shell2 C++

## What changed after validating the TRUEOS path

The browser prototype was originally accompanied by generic OpenCL C image
kernels (`image3d_t`, samplers, 4x4x4 simulation groups, 8x8 render groups).
That was a useful portability sketch, but it does **not** resemble the actual
TRUEOS execution contract closely enough.

The current TRUEOS Shell2 `cpp` lineage is the better target:

```text
C++ for OpenCL source
        |
        | offline build
        v
LLVM / SPIR-V
        |
        v
Intel IGC / ocloc
        |
        v
reviewed Zebin + generated ABI contract
        |
        v
TRUEOS direct RCS + GuC
        |
        v
SIMD16 GPGPU_WALKER -> linear UI4 RGBA8 surface
```

The existing TRUEOS C++ demo path already performs the important runtime work:
artifact admission, PPGTT mapping, generated value-contract packing, interface
descriptor/binding setup, SIMD16 local-ID payload construction, walker launch,
completion-marker polling, and UI4 surface publication.

Therefore the cloud port should extend the **Shell2 C++ path**, not the narrow
ShaderToy 64-byte ABI and not a generic OpenCL image runtime abstraction.

## Browser remains the Linux visual reference

Nothing about this alignment changes how the web demo looks or behaves.

```bash
python3 serve.py --open
```

The browser continues to use:

- WebGPU/WGSL;
- two `rgba16float` 3D textures;
- 4x4x4 simulation workgroups;
- fullscreen ray marching;
- hardware-filtered 3D sampling;
- the full control panel and JSON config I/O.

Those are authoring/runtime details of the web implementation. They are no
longer treated as requirements for the TRUEOS command encoder.

## Canonical TRUEOS-compatible kernels

The native target now lives in:

```text
native/trueos/cloud_simulation.clcpp
native/trueos/cloud_render.clcpp
```

Both are C++ for OpenCL and require:

```text
subgroup:   SIMD16
local size: 16 x 1 x 1
```

This matches the existing direct-RCS Shell2 C++ walker shape: one 16-lane local
row is one hardware SIMD16 thread.

### Simulation

The 96x48x96 volume is flattened into one linear voxel domain:

```text
voxel_count = 96 * 48 * 96 = 442,368
local_size  = 16
workgroups  = ceil(442,368 / 16) = 27,648
```

Each lane reconstructs x/y/z from `get_global_id(0)`.

The volume is a persistent linear `half4` allocation. Two copies ping-pong:

```text
volume A -> cloud_simulation -> volume B
volume B -> cloud_simulation -> volume A
```

Each volume is 3,538,944 bytes; the pair is 7,077,888 bytes (6.75 MiB).

### Render

The render kernel writes packed RGBA8 directly into a linear destination using
exactly the row-oriented geometry already natural to TRUEOS:

```text
local_size = 16 x 1 x 1
groups     = ceil(width / 16) x height x 1
```

At 1920x1080 that is `120 x 1080 x 1` groups.

No graphics pipeline or geometry is needed.

## First compatibility contract: software trilinear volume sampling

The browser uses a hardware-filtered 3D texture. TRUEOS does not need to gain a
complete texture/sampler API before the cloud can exist.

The first native contract therefore stores the volume in ordinary persistent
`half4` buffers and implements trilinear interpolation in the kernels. This is
slower than sampler hardware, but it is valuable because it isolates the first
parity target to things TRUEOS already handles well:

- persistent GPU memory;
- pointer/buffer kernel arguments;
- SIMD16 execution;
- generated C++ for OpenCL ABI contracts;
- producer/consumer ordering;
- linear RGBA8 output.

Once the buffer version matches the browser mathematically, a hardware 3D
sampler can replace the sampling helper as a focused performance upgrade. The
JSON format, parameter ABI, simulation model, and frame scheduling do not need
to change for that upgrade.

## Intended TRUEOS frame batch

The final runtime should not submit and CPU-poll between the two cloud stages.
The desired frame is one retained multi-walker batch:

```text
update SimParams / RenderParams
        |
        v
walker 0: cloud_simulation
        |
        v
GPU producer -> consumer memory dependency
        |
        v
walker 1: cloud_render
        |
        v
flush + one final completion marker
        |
        v
UI4 producer release / publish
```

When paused, skip the simulation walker unless a brush edit/reset must be
applied. Rendering can continue from the retained current volume.

`native/cloud_dispatch_plan.hpp` encodes this intended geometry without trying
to duplicate TRUEOS's RCS command builder in the portable example.

## Parameter ABI

The browser and native model retain the same fixed POD sizes:

```text
SimParams:    112 bytes
RenderParams: 272 bytes
```

See `native/cloud_params.hpp`.

The important separation is:

```text
large browser control panel
        |
        v
versioned cloud-engine-config JSON
        |
        v
host validation / conversion
        |
        +--> SimParams
        +--> RenderParams
        |
        v
AOT kernels
```

The native kernels never parse JSON. TRUEOS only needs the final fixed structs
and retained resource handles.

The JSON still contains every authored setting, including camera pose, sun,
quality, palette, artistic controls, air motion, brush settings, formation, and
pause state. The mutable density texture itself is intentionally not serialized.

## Build-time vs runtime responsibility

### Offline / authoring

- C++ for OpenCL source is reviewed.
- It is compiled to SPIR-V with the LLVM/OpenCL-Clang toolchain paired to the
  selected Intel IGC revision.
- IGC/ocloc produces the exact-target Zebin.
- ABI metadata/hashes are generated and compiled into the trusted catalog.

### Runtime

- select the reviewed cloud artifacts;
- allocate/map two persistent volume buffers;
- map the UI4 destination;
- fill generated kernel payloads from `SimParams`/`RenderParams`;
- encode simulation + dependency + render;
- observe only the final retirement marker;
- publish the finished UI4 frame.

No runtime shader compiler is required.

## Exact-target note

Do not treat one Intel Xe-family Zebin as universally interchangeable. The
portable source/SPIR-V layer can be shared, but production artifacts should be
baked and admitted for the exact TRUEOS target/device family just like the
existing C++ path.

The older Tiger Lake laptop is useful evidence that the workload has ample
headroom on an older Xe-LP iGPU. It does not need to become the primary TRUEOS
binary target for the compatibility work to be useful.

## Validation strategy for parity

Compare the browser against the native path in layers:

1. deterministic config/seed;
2. one volume Z slice rendered as grayscale;
3. software-trilinear sampling of a known 3D gradient;
4. density-only ray march with lighting disabled;
5. realistic lighting;
6. artistic mode;
7. animation/advection;
8. brush edits and pause behavior.

For TRUEOS visual comparisons, capture the exact published UI4 RGBA surface
first. Compare display-writeback/post-blend output only after shader parity is
established, because the latter adds another color/presentation path.

## Source relationship

```text
Browser visual reference
  shaders/simulate.wgsl
  shaders/render.wgsl

TRUEOS execution reference
  native/trueos/cloud_simulation.clcpp
  native/trueos/cloud_render.clcpp
  native/cloud_params.hpp
  native/cloud_dispatch_plan.hpp

Historical generic-image reference only
  native/reference_opencl_c/cloud_simulation_image3d.cl
  native/reference_opencl_c/cloud_render_image3d.cl
```

The WGSL and C++ for OpenCL implementations should stay numerically close, but
bit identity is not expected because texture filtering, math libraries, and
compiler lowering differ.
