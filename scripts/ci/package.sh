#!/usr/bin/env bash
set -euo pipefail
arch="$1"
version="$(./target/release/y5 --version | cut -d' ' -f2)"
mkdir -p release-assets stage/usr/bin stage/DEBIAN
install -m755 target/release/y5 stage/usr/bin/y5
glibc="$(ldd --version | sed -n '1p' | grep -oE '[0-9]+\.[0-9]+$')"
cat > stage/DEBIAN/control <<EOF
Package: y5
Version: $version
Architecture: $arch
Maintainer: Y5 contributors
Description: Terminal Wi-Fi audit console for resource-constrained Linux systems
Depends: libc6 (>= $glibc)
Recommends: aircrack-ng, iw, iproute2
Suggests: hashcat, hcxtools, hcxdumptool, kismet, wifite
EOF
dpkg-deb --build --root-owner-group stage "release-assets/y5_${version}_${arch}.deb"
tar -czf "release-assets/y5_${version}_${arch}.tar.gz" -C target/release y5 -C "$PWD" README.md
(cd release-assets && sha256sum *.deb *.tar.gz > "SHA256SUMS-${arch}")
