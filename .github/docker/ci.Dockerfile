# The toolchain every CI job needs, built once and published, instead of reassembled per job.
#
# A verify job used to spend minutes before it ran anything: apt, a libmgba source build, a rustup
# toolchain, `cargo install agb-gbafix`. Multiplied by ~90 matrix jobs that is the bulk of the
# pipeline, and none of it changes between runs.
#
# Rebuilt only when this file changes (see .github/workflows/ci-image.yml) and published to
# ghcr.io/schlopai/chuggie-ci.
#
# NOT in here, deliberately:
#   - node_modules      pinned by package-lock.json, which changes with the repo; `npm ci` per job
#   - the agb fork      pinned by rev in .cargo/config.toml; cargo fetches and caches it
#   - the repo itself   obviously
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/node/bin:$PATH

# The same package list the workflows installed by hand. freetype is for the host-side asset bake;
# imagemagick, numpy and pillow are what the verifier scripts use to inspect frames.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates cmake curl git jq pkg-config unzip xz-utils \
      imagemagick python3 python3-numpy python3-pil \
      libfreetype6-dev libpng-dev zlib1g-dev \
 && rm -rf /var/lib/apt/lists/*

# libmgba 0.10.5. PINNED: the verifiers assert on exact frames and exact audio, so the version is
# part of the contract — distro packages are not interchangeable here. Same cmake flags the
# workflows used, so the build is headless and drags in no Qt/SDL/FFmpeg.
RUN git clone --depth 1 --branch 0.10.5 https://github.com/mgba-emu/mgba /tmp/mgba-src \
 && cmake -S /tmp/mgba-src -B /tmp/mgba-build -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX=/usr/local/mgba -DBUILD_QT=OFF -DBUILD_SDL=OFF \
      -DUSE_FFMPEG=OFF -DUSE_SQLITE3=OFF -DUSE_DISCORD_RPC=OFF -DUSE_EDITLINE=OFF \
      -DUSE_ELF=OFF -DUSE_EPOXY=OFF -DBUILD_GL=OFF -DBUILD_GLES2=OFF \
 && make -C /tmp/mgba-build -j"$(nproc)" install \
 && rm -rf /tmp/mgba-src /tmp/mgba-build
ENV MGBA_PREFIX=/usr/local/mgba \
    LD_LIBRARY_PATH=/usr/local/mgba/lib

# The SAME pinned nightly rust-toolchain.toml names — a floating nightly changes ROM codegen and
# shifts frame-exact verifier timings.
ARG RUST_TOOLCHAIN=nightly-2026-07-22
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --default-toolchain "$RUST_TOOLCHAIN" --component rust-src \
 && rustup target add thumbv4t-none-eabi || true

# Stable as well: the host crate tests, rustfmt and clippy run on it, and pulling a second toolchain
# per job is the same cost this image exists to remove.
RUN rustup toolchain install stable --component rustfmt,clippy --profile minimal

# ELF -> .gba. `cargo install` compiles it, which is exactly the kind of per-job cost this image exists to remove.
RUN cargo install --locked agb-gbafix

ARG NODE_VERSION=24.9.0
RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ -C /usr/local && mv "/usr/local/node-v${NODE_VERSION}-linux-x64" /usr/local/node

# Fail the image build rather than every job that uses it.
RUN rustc --version && cargo --version && node --version && npm --version \
 && agb-gbafix --version 2>/dev/null || true
RUN test -f /usr/local/mgba/include/mgba/core/core.h && test -d /usr/local/mgba/lib
