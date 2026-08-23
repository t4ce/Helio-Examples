#include "cloud_dispatch_plan.hpp"

#include <iostream>

int main() {
  using namespace cloud_engine;

  std::cout << "SimParams: " << sizeof(SimParams) << " bytes\n";
  std::cout << "RenderParams: " << sizeof(RenderParams) << " bytes\n";
  const auto sim = simulation_dispatch();
  const auto render1080 = render_dispatch(1920, 1080);
  std::cout << "TRUEOS simulation groups: " << sim.groups_x << " x "
            << sim.groups_y << " x " << sim.groups_z << " (local 16x1x1)\n";
  std::cout << "TRUEOS 1920x1080 render groups: " << render1080.groups_x << " x "
            << render1080.groups_y << " x " << render1080.groups_z
            << " (local 16x1x1)\n";
  std::cout << "Ping-pong volume bytes: " << kPingPongVolumeBytes << "\n";
  return 0;
}
