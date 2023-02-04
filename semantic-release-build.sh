#!/bin/bash

VERSION="$1"

cargo install cross cargo-deb
sed -i '/\[package\]/,/^version = "[^"]*"$/ s/^version = "[^"]*"$/version = "'"$VERSION"'"/' Cargo.toml
cross build --target arm-unknown-linux-gnueabihf --release || exit 1
cargo-deb -v --no-build --target arm-unknown-linux-gnueabihf --no-strip || exit 1

# This is workaround for https://github.com/kornelski/cargo-deb/issues/47
# Patch the generated DEB to have ./ paths compatible with `unattended-upgrade`:
pushd target/arm-unknown-linux-gnueabihf/debian || exit 1
DEB_FILE_NAME=$(ls -1 *.deb | head -n 1)
DATA_ARCHIVE=$(ar t "${DEB_FILE_NAME}"| grep -E '^data\.tar')
ar x "${DEB_FILE_NAME}" "${DATA_ARCHIVE}"
tar tf "${DATA_ARCHIVE}"

if [[ "${DATA_ARCHIVE}" == *.xz ]]; then
  # Install XZ support that will be needed by TAR
  apt-get -y install -y xz-utils
  EXTRA_TAR_ARGS=J
fi

mkdir tar-hack
tar -C tar-hack -xf "${DATA_ARCHIVE}"
pushd tar-hack || exit 1
tar c${EXTRA_TAR_ARGS}f "../${DATA_ARCHIVE}" --owner=0 --group=0 ./*
popd || exit 1
tar tf "${DATA_ARCHIVE}"
ar r "${DEB_FILE_NAME}" "${DATA_ARCHIVE}"
popd || exit 1
