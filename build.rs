//! Build script: compiles the cycfi/q C++ pitch-detector wrapper.
//!
//! Prerequisites (fulfilled via git submodules):
//!   vendor/q     — https://github.com/cycfi/q     (pitch detector, MIT)
//!   vendor/infra — https://github.com/cycfi/infra (Q's support library, MIT)
//!
//! If the submodules are not initialised, run:
//!   git submodule update --init --recursive

use std::path::Path;

fn main() {
    let q_include = "vendor/q/q_lib/include";
    let infra_include = "vendor/infra/include";

    // Friendly error if submodules haven't been initialised.
    for dir in &[q_include, infra_include] {
        if !Path::new(dir).exists() {
            panic!(
                "\n\n\
                 ── cycfi/q vendor submodules not found ─────────────────────\n\
                 Expected directory: {dir}\n\
                 Run:  git submodule update --init --recursive\n\
                 ────────────────────────────────────────────────────────────\n"
            );
        }
    }

    // Re-run only when the wrapper source changes (not on every Q header edit).
    println!("cargo:rerun-if-changed=src/q_wrapper.cpp");
    println!("cargo:rerun-if-changed=src/q_wrapper.hpp");

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include(q_include)
        .include(infra_include)
        .file("src/q_wrapper.cpp")
        // Suppress warnings from third-party headers.
        .warnings(false)
        .compile("q_wrapper");
}
