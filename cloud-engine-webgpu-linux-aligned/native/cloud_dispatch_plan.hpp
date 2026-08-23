#pragma once

#include "cloud_params.hpp"

#include <cstddef>
#include <cstdint>

namespace cloud_engine {

// TRUEOS's current Shell2 C++ path launches one SIMD16 hardware thread per
// logical group. Keep both cloud kernels in that exact execution shape.
inline constexpr std::uint32_t kNativeLocalSizeX = 16;
inline constexpr std::uint32_t kNativeLocalSizeY = 1;
inline constexpr std::uint32_t kNativeLocalSizeZ = 1;

inline constexpr std::uint32_t kVolumeChannels = 4;
inline constexpr std::uint32_t kVolumeComponentBytes = 2; // half
inline constexpr std::uint64_t kVolumeVoxelCount =
    static_cast<std::uint64_t>(kVolumeWidth) * kVolumeHeight * kVolumeDepth;
inline constexpr std::uint64_t kVolumeBytes =
    kVolumeVoxelCount * kVolumeChannels * kVolumeComponentBytes;
inline constexpr std::uint64_t kPingPongVolumeBytes = kVolumeBytes * 2;

struct Dispatch3D {
  std::uint32_t groups_x;
  std::uint32_t groups_y;
  std::uint32_t groups_z;
};

[[nodiscard]] constexpr Dispatch3D simulation_dispatch() noexcept {
  return {
      divide_round_up(static_cast<std::uint32_t>(kVolumeVoxelCount), kNativeLocalSizeX),
      1,
      1,
  };
}

[[nodiscard]] constexpr Dispatch3D render_dispatch(
    const std::uint32_t width,
    const std::uint32_t height) noexcept {
  return {
      divide_round_up(width, kNativeLocalSizeX),
      height,
      1,
  };
}

// Target frame order. TRUEOS can encode these as two walkers in one RCS batch:
//   simulate -> producer/consumer memory dependency -> render -> final marker.
// The CPU only needs to observe the final completion marker before UI4 release.
enum class CloudFrameStage : std::uint32_t {
  Simulation = 0,
  SimulationToRenderBarrier = 1,
  Render = 2,
  FinalCompletionMarker = 3,
};

static_assert(kVolumeVoxelCount == 442368);
static_assert(kVolumeBytes == 3538944);
static_assert(kPingPongVolumeBytes == 7077888);
static_assert(simulation_dispatch().groups_x == 27648);

} // namespace cloud_engine
