default:
    just --choose

run:
    cargo r

setup:
    cargo install cross

cross-compile:
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-unknown-linux-gnu-gcc cargo build --target=x86_64-unknown-linux-gnu