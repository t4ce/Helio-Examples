#pragma once

#include <cstddef>
#include <cstdint>
#include <type_traits>

namespace cloud_engine {

struct alignas(16) Float4 {
  float x;
  float y;
  float z;
  float w;
};

static_assert(sizeof(Float4) == 16);
static_assert(alignof(Float4) == 16);
static_assert(std::is_standard_layout_v<Float4>);
static_assert(std::is_trivially_copyable_v<Float4>);

enum class Formation : std::uint32_t {
  CumulusField = 0,
  LayeredDeck = 1,
  HighWisps = 2,
  StormTowers = 3,
  BrokenCells = 4,
  MoonScrolls = 5,
  WindRibbons = 6,
};

// Byte-for-byte match for shaders/simulate.wgsl::SimParams and
// native/cloud_simulation.cl::SimParams.
struct alignas(16) SimParams {
  Float4 time_step;           // dt, time, amount, auto mode
  Float4 wind_turbulence;     // world-space wind xyz, turbulence
  Float4 brush_center_radius; // normalized xyz, radius relative to X extent
  Float4 brush_controls;      // strength, active, sign, pattern id
  Float4 flow_controls;       // dissipation, tearing, rotation, clear pulse
  Float4 volume_seed;         // dimensions xyz stored as floats, seed
  Float4 art_controls;        // artistic mode, curl, ribbon stretch, sculpt
};

// Byte-for-byte match for shaders/render.wgsl::RenderParams and
// native/cloud_render.cl::RenderParams.
struct alignas(16) RenderParams {
  Float4 resolution_time;
  Float4 camera_position_tan_fov;
  Float4 camera_forward_exposure;
  Float4 camera_right_steps;
  Float4 camera_up_detail;
  Float4 sun_direction_intensity;
  Float4 sun_color_extinction;
  Float4 sky_top_ambient;
  Float4 sky_horizon_seed;
  Float4 bounds_min_density;
  Float4 bounds_max_shadow;
  Float4 options;
  Float4 art_style;        // enabled, toon bands, outline, sculpt
  Float4 art_cloud_color;  // linear RGB, print grain
  Float4 art_shadow_color; // linear RGB, ribbon parameter
  Float4 art_sky_color;    // linear RGB, moon angular radius
  Float4 art_moon_color;   // linear RGB, moon glow
};

inline constexpr std::uint32_t kVolumeWidth = 96;
inline constexpr std::uint32_t kVolumeHeight = 48;
inline constexpr std::uint32_t kVolumeDepth = 96;
inline constexpr std::uint32_t kSimulationWorkgroupX = 4;
inline constexpr std::uint32_t kSimulationWorkgroupY = 4;
inline constexpr std::uint32_t kSimulationWorkgroupZ = 4;
inline constexpr std::uint32_t kRenderWorkgroupX = 8;
inline constexpr std::uint32_t kRenderWorkgroupY = 8;

static_assert(sizeof(SimParams) == 112);
static_assert(alignof(SimParams) == 16);
static_assert(offsetof(SimParams, volume_seed) == 80);
static_assert(offsetof(SimParams, art_controls) == 96);
static_assert(std::is_standard_layout_v<SimParams>);
static_assert(std::is_trivially_copyable_v<SimParams>);

static_assert(sizeof(RenderParams) == 272);
static_assert(alignof(RenderParams) == 16);
static_assert(offsetof(RenderParams, options) == 176);
static_assert(offsetof(RenderParams, art_style) == 192);
static_assert(offsetof(RenderParams, art_cloud_color) == 208);
static_assert(offsetof(RenderParams, art_shadow_color) == 224);
static_assert(offsetof(RenderParams, art_sky_color) == 240);
static_assert(offsetof(RenderParams, art_moon_color) == 256);
static_assert(std::is_standard_layout_v<RenderParams>);
static_assert(std::is_trivially_copyable_v<RenderParams>);

[[nodiscard]] constexpr std::uint32_t divide_round_up(
    std::uint32_t value,
    std::uint32_t divisor) noexcept {
  return (value + divisor - 1u) / divisor;
}

} // namespace cloud_engine
