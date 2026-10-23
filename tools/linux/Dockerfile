# Compilation de ManageComp pour Linux x86-64 (PC), lancée par tools/release.py.
# Ubuntu 24.04 : GTK 4.14 (le logiciel demande GTK 4.12 ou plus récent).
FROM ubuntu:24.04
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl build-essential pkg-config libgtk-4-dev libudev-dev \
 && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
# Remplace les chemins GTK de Windows fixés dans .cargo/config.toml.
ENV PKG_CONFIG=pkg-config PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig
WORKDIR /src
