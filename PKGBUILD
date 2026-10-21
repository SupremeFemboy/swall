# Maintainer: Supreme Femboy
pkgname=swall-git
_pkgname=swall
pkgver=0.1.0
pkgrel=1
pkgdesc="Fast GPU-accelerated Wayland layer-shell wallpaper selector with 16:9 parallelograms"
arch=('x86_64')
url="https://github.com/RustBlyat/swall"
license=('MIT' 'Apache-2.0')
depends=('gcc-libs' 'glibc' 'fontconfig' 'wayland')
makedepends=('cargo' 'git')
optdepends=(
    'swww: Wayland wallpaper daemon'
    'hyprpaper: Hyprland wallpaper utility'
    'swaybg: Wallpaper utility for wlroots'
    'vulkan-driver: Hardware-accelerated GPU rendering'
)
provides=("$_pkgname")
conflicts=("$_pkgname")
source=("git+$url.git")
sha256sums=('SKIP')

_get_workdir() {
    if [ -d "$srcdir/$_pkgname" ]; then
        echo "$srcdir/$_pkgname"
    else
        echo "$startdir"
    fi
}

pkgver() {
    cd "$(_get_workdir)"
    if git rev-parse --git-dir >/dev/null 2>&1 && [ -n "$(git rev-parse --short HEAD 2>/dev/null)" ]; then
        if git describe --long --tags --always >/dev/null 2>&1; then
            git describe --long --tags --always | sed 's/\([^-]*-g\)/r\1/;s/-/./g'
        else
            printf "r%s.g%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
        fi
    else
        echo "0.1.0"
    fi
}

prepare() {
    cd "$(_get_workdir)"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$(_get_workdir)"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release
}

check() {
    cd "$(_get_workdir)"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo test --frozen
}

package() {
    cd "$(_get_workdir)"
    install -Dm755 "target/release/$_pkgname" "$pkgdir/usr/bin/$_pkgname"
    install -Dm644 "README.md" "$pkgdir/usr/share/doc/$_pkgname/README.md"
}
