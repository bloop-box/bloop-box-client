FROM ghcr.io/cross-rs/arm-unknown-linux-gnueabihf:main

RUN dpkg --add-architecture armhf && \
    apt-get update && \
    apt-get install --assume-yes gcc-multilib clang

ENV PKG_CONFIG_LIBDIR=/usr/local/lib/arm-linux-gnueabihf/pkgconfig:/usr/lib/arm-linux-gnueabihf/pkgconfig
ENV CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-L /usr/lib/arm-linux-gnueabihf $CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS"
ENV CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-C link-args=-Wl,-rpath-link,/usr/lib/arm-linux-gnueabihf $CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS"
