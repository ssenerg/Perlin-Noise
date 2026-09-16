# Rendered images land here.
out := "images"

# List the recipes.
default:
    @just --list --unsorted

# Render one PNG into images/, e.g. `just image relief --seed 7 --cells 12`.
image mode="relief" *flags:
    cargo run --quiet --release --example image --features png -- \
        --mode {{mode}} --out {{out}}/{{mode}}.png {{flags}}

# 4K relief wallpaper, e.g. `just wallpaper 1 --palette azure`. 3841x2161.
wallpaper seed="1" *flags:
    cargo run --quiet --release --example image --features png -- \
        --mode relief --cells 9,16 --detail 240 --seed {{seed}} \
        --out {{out}}/wallpaper-4k-{{seed}}.png {{flags}}

# The same 4K picture square instead, 3841x3841.
wallpaper-square seed="1" *flags:
    cargo run --quiet --release --example image --features png -- \
        --mode relief --cells 8 --detail 480 --seed {{seed}} \
        --out {{out}}/wallpaper-4k-square-{{seed}}.png {{flags}}

# A lit sphere out of a 4D field, e.g. `just globe 3 --palette crimson`.
globe seed="1" *flags:
    cargo run --quiet --release --example image --features png -- \
        --mode globe --size 1440 --seed {{seed}} \
        --out {{out}}/globe-{{seed}}.png {{flags}}

# The glossy liquid look at screen size, one file per seed.
liquid seed="1" *flags:
    cargo run --quiet --release --example image --features png -- \
        --mode relief --cells 9,16 --seed {{seed}} \
        --out {{out}}/liquid-{{seed}}.png {{flags}}

# Every mode at once, for comparing them side by side.
gallery:
    cargo run --quiet --release --example render --features png -- {{out}}

# Every mode and option the renderer takes.
options:
    @cargo run --quiet --release --example image --features png -- --help

# Open a rendered image (macOS).
show file:
    open {{file}}

# Run the tests, GPU and PNG paths included.
test:
    cargo test --features "png gpu"

# Add the slow multi-dispatch GPU test.
test-all:
    cargo test --release --features "png gpu" -- --include-ignored

# Lint every feature combination, warnings included.
lint:
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --features png -- -D warnings
    cargo clippy --all-targets --features gpu -- -D warnings
    cargo clippy --all-targets --features "png gpu" -- -D warnings

# Format the code.
fmt:
    cargo fmt

# What to run before committing.
check: fmt lint test

# Drop build output and rendered images.
clean:
    cargo clean
    rm -rf {{out}}
