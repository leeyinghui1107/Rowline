cmake_minimum_required(VERSION 3.16)
project(winlinez_reference_c C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

option(WL_BUILD_APP "Build the SDL2 application" ON)

# ---------------------------------------------------------------- model
add_library(wl_model STATIC wl_model.c)
target_include_directories(wl_model PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

# ---------------------------------------------------------------- replay CLI
add_executable(wlinez-replay wl_replay.c)
target_link_libraries(wlinez-replay PRIVATE wl_model)

# ---------------------------------------------------------------- layout gate
add_executable(wlinez-layout-check wl_layout_check.c wl_layout.c)
target_link_libraries(wlinez-layout-check PRIVATE wl_model)

# ---------------------------------------------------------------- SDL2 app
if(WL_BUILD_APP)
    find_package(SDL2 QUIET)
    if(SDL2_FOUND)
        add_executable(wlinez wl_main.c wl_layout.c wl_render.c)
        target_link_libraries(wlinez PRIVATE wl_model)
        if(TARGET SDL2::SDL2)
            target_link_libraries(wlinez PRIVATE SDL2::SDL2)
            target_include_directories(wlinez PRIVATE ${SDL2_INCLUDE_DIRS})
        else()
            target_include_directories(wlinez PRIVATE ${SDL2_INCLUDE_DIRS})
            target_link_libraries(wlinez PRIVATE ${SDL2_LIBRARIES})
        endif()
    else()
        message(WARNING "SDL2 not found - skipping the graphical target")
    endif()
endif()

enable_testing()
add_test(NAME layout_check COMMAND wlinez-layout-check)
# The differential gate lives in the Rust workspace; it is driven from
# `make test` at the repository root.
