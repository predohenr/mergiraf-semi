load("@rules_rust//rust:defs.bzl", "rust_library")

rust_library(
    name = "lib",
    srcs = ["src/lib.rs"],
    deps = [
        "@crates//:anyhow",
        "@crates//:log",
        "@crates//:serde",
    ],
)