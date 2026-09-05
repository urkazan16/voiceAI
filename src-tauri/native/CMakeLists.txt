cmake_minimum_required(VERSION 3.16)
project(localflow_runtime C)

set(CMAKE_C_STANDARD 11)

add_library(localflow_runtime STATIC src/runtime.c)
target_include_directories(localflow_runtime PUBLIC include)

# Optional verified MIT subset of whisper.cpp / llama.cpp is never pulled from
# unreviewed examples, bindings, or extra codec directories.
if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/../../third_party/whisper.cpp/CMakeLists.txt")
  message(STATUS "third_party whisper.cpp present — not auto-linked; use scripts/build-native-runtime.sh")
endif()
