load("@rules_rust//rust:defs.bzl", "rust_library")

rust_library(
    name = "lib",
    srcs = ["src/lib.rs"],
    deps = [
        "@crates//:log",
        "@crates//:serde",
    ],
)