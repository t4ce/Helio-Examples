cmake_minimum_required(VERSION 3.20)
project(cloud_engine_native_contract LANGUAGES CXX)

add_executable(cloud_abi_check abi_check.cpp)
target_compile_features(cloud_abi_check PRIVATE cxx_std_20)
target_compile_options(cloud_abi_check PRIVATE
  $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall;-Wextra;-Wpedantic;-Werror>
)
