#!/bin/bash

VERSION="$1"

cargo install cross cargo-deb
sed -i '/\[package\]/,/^version = "[^"]*"$/ s/^version = "[^"]*"$/version = "'"$VERSION"'"/' Cargo.toml
cross build --target arm-unknown-linux-gnueabihf --release
cargo-deb -v --no-build --target arm-unknown-linux-gnueabihf --no-strip
