cmake_minimum_required(VERSION 3.16)
project(localflow_native C)

set(CMAKE_C_STANDARD 11)

# The former src/runtime.c stub is removed. STT is whisper-rs; Speech.framework
# is compiled from build.rs (speech.m), not this CMake target.
add_library(localflow_native INTERFACE)
target_include_directories(localflow_native INTERFACE include)
