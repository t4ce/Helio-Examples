#include "cloud_params.hpp"

#include <iostream>

int main() {
  using namespace cloud_engine;

  std::cout << "SimParams: " << sizeof(SimParams) << " bytes\n";
  std::cout << "RenderParams: " << sizeof(RenderParams) << " bytes\n";
  std::cout << "Simulation dispatch: "
            << divide_round_up(kVolumeWidth, kSimulationWorkgroupX) << " x "
            << divide_round_up(kVolumeHeight, kSimulationWorkgroupY) << " x "
            << divide_round_up(kVolumeDepth, kSimulationWorkgroupZ) << "\n";
  return 0;
}
