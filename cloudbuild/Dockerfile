# NevoFlux Cloud Build - Custom Builder Image
#
# Pre-installs all dependencies needed for Linux and Windows cross-compilation
# builds of the NevoFlux browser (Firefox/Zen-based).
#
# Includes pre-installed Wine + VS2022 cross-compile toolchain so that
# the `windows-setup` build step is skipped entirely (saves ~15-20 min).
#
# Build:
#   docker build -t gcr.io/PROJECT_ID/nevoflux-builder:latest cloudbuild/
#
# Push:
#   docker push gcr.io/PROJECT_ID/nevoflux-builder:latest
#
# NOTE: When updating the Firefox/engine version, re-copy the VS2022 manifest:
#   cp engine/build/vs/vs2022.yaml cloudbuild/win-cross/vs2022.yaml
#   cp engine/third_party/python/vsdownload/vsdownload.py cloudbuild/win-cross/vsdownload.py
#   Then rebuild and push the image.
#
# This image is used by Cloud Build configs in cloudbuild/*.yaml

FROM ubuntu:22.04

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------
ENV DEBIAN_FRONTEND=noninteractive \
  LANG=C.UTF-8 \
  LC_ALL=C.UTF-8 \
  RUSTUP_HOME=/usr/local/rustup \
  CARGO_HOME=/usr/local/cargo \
  PATH="/usr/local/cargo/bin:/usr/local/google-cloud-sdk/bin:${PATH}"

# ---------------------------------------------------------------------------
# Core system packages (matches GitHub Actions CI)
# ---------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
  python3 \
  python3-pip \
  python3-launchpadlib \
  python3-requests \
  python3-toml \
  dos2unix \
  yasm \
  nasm \
  build-essential \
  libgtk2.0-dev \
  libpython3-dev \
  m4 \
  uuid \
  libasound2-dev \
  libcurl4-openssl-dev \
  libdbus-1-dev \
  libdrm-dev \
  libdbus-glib-1-dev \
  libgtk-3-dev \
  libpulse-dev \
  libx11-xcb-dev \
  libxt-dev \
  xvfb \
  lld \
  llvm \
  git \
  curl \
  wget \
  zip \
  unzip \
  jq \
  autoconf \
  autoconf2.13 \
  automake \
  bison \
  cmake \
  flex \
  gawk \
  gcc-multilib \
  gnupg \
  libbz2-dev \
  libexpat1-dev \
  libffi-dev \
  libncursesw5-dev \
  libsqlite3-dev \
  libssl-dev \
  libtool \
  libucl-dev \
  libxml2-dev \
  msitools \
  ninja-build \
  openssh-client \
  p7zip-full \
  pkg-config \
  procps \
  scons \
  subversion \
  tar \
  uuid-dev \
  zlib1g-dev \
  aria2 \
  libfuse2 \
  desktop-file-utils \
  appstream \
  zstd \
  ca-certificates \
  software-properties-common \
  && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Node.js 20 (via NodeSource)
# ---------------------------------------------------------------------------
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
  && apt-get install -y --no-install-recommends nodejs \
  && rm -rf /var/lib/apt/lists/* \
  && node --version && npm --version

# ---------------------------------------------------------------------------
# Rust 1.83 with cross-compilation targets
# ---------------------------------------------------------------------------
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --default-toolchain 1.83 \
  && rustup target add \
    x86_64-unknown-linux-gnu \
    aarch64-unknown-linux-gnu \
    x86_64-pc-windows-msvc \
    aarch64-pc-windows-msvc \
  && rustc --version && cargo --version

# ---------------------------------------------------------------------------
# GitHub CLI (gh)
# ---------------------------------------------------------------------------
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
  | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
  && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
  && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list \
  && apt-get update && apt-get install -y --no-install-recommends gh \
  && rm -rf /var/lib/apt/lists/* \
  && gh --version

# ---------------------------------------------------------------------------
# Google Cloud SDK (provides gsutil, gcloud)
# ---------------------------------------------------------------------------
RUN curl -fsSL https://packages.cloud.google.com/apt/doc/apt-key.gpg \
  | gpg --dearmor -o /usr/share/keyrings/cloud.google.gpg \
  && echo "deb [signed-by=/usr/share/keyrings/cloud.google.gpg] https://packages.cloud.google.com/apt cloud-sdk main" \
    > /etc/apt/sources.list.d/google-cloud-sdk.list \
  && apt-get update && apt-get install -y --no-install-recommends google-cloud-sdk \
  && rm -rf /var/lib/apt/lists/* \
  && gcloud --version

# ---------------------------------------------------------------------------
# sccache (compile cache with GCS backend)
# ---------------------------------------------------------------------------
RUN SCCACHE_VERSION="v0.9.1" \
  && curl -fsSL "https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
  | tar xz --strip-components=1 -C /usr/local/bin/ "sccache-${SCCACHE_VERSION}-x86_64-unknown-linux-musl/sccache" \
  && chmod +x /usr/local/bin/sccache \
  && sccache --version

# ---------------------------------------------------------------------------
# @zen-browser/surfer (global npm package)
# ---------------------------------------------------------------------------
RUN npm install -g @zen-browser/surfer

# ---------------------------------------------------------------------------
# Python packages for VS2022 installer
# ---------------------------------------------------------------------------
RUN pip3 install --no-cache-dir certifi pyyaml six

# ---------------------------------------------------------------------------
# Wine (Mozilla cross-compile toolchain for Windows builds)
# ---------------------------------------------------------------------------
RUN mkdir -p /root/win-cross \
  && aria2c "https://firefox-ci-tc.services.mozilla.com/api/index/v1/task/gecko.cache.level-1.toolchains.v3.linux64-wine.latest/artifacts/public%2Fbuild%2Fwine.tar.zst" \
    -o /tmp/wine.tar.zst \
  && zstd -d /tmp/wine.tar.zst -o /tmp/wine.tar \
  && tar xf /tmp/wine.tar -C /root/win-cross \
  && rm -f /tmp/wine.tar.zst /tmp/wine.tar \
  && ls -d /root/win-cross/wine

# ---------------------------------------------------------------------------
# VS2022 cross-compile toolchain (downloaded via Mozilla's vsdownload)
# ---------------------------------------------------------------------------
COPY win-cross/vsdownload.py /tmp/win-cross/vsdownload.py
COPY win-cross/vs2022.yaml /tmp/win-cross/vs2022.yaml
COPY win-cross/install-vs.py /tmp/win-cross/install-vs.py

RUN cd /tmp/win-cross \
  && python3 install-vs.py vs2022.yaml /root/win-cross/vs2022 \
  && rm -rf /tmp/win-cross \
  && ls -d /root/win-cross/vs2022

# ---------------------------------------------------------------------------
# Workspace
# ---------------------------------------------------------------------------
WORKDIR /workspace
