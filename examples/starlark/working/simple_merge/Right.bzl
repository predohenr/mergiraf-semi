load("@rules_rust//rust:defs.bzl", "rust_library")
load("@rules_cc//cc:defs.bzl", "cc_library")

rust_library(
    name = "lib",
    srcs = ["src/lib.rs"],
    deps = [
        "@crates//:log",
        "@crates//:serde",
    ],
)