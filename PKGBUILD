# Maintainer: user12ms <user12ms@localhost>
pkgname=sfc-coreutils
pkgver=0.1.0
pkgrel=1
pkgdesc="Simple & Fast Coreutils in Rust"
arch=('x86_64')
url="https://github.com/user12msd4c/sfc"
license=('MIT')
depends=('glibc' 'gcc-libs')
makedepends=('cargo')
conflicts=('coreutils')
provides=('coreutils')
source=("git+file://$PWD/..") # Для локальной сборки
sha256sums=('SKIP')

build() {
    cd "$srcdir/sfc"
    cargo build --release --locked --target-dir target
}

package() {
    cd "$srcdir/sfc"

    install -d "$pkgdir/usr/bin"
    for bin in target/release/*; do
        if [ -f "$bin" ] && [ -x "$bin" ]; then
            filename=$(basename "$bin")
            if [[ "$filename" != *.* ]]; then
                install -Dm755 "$bin" "$pkgdir/usr/bin/$filename"
            fi
        fi
    done

    ln -sf ls "$pkgdir/usr/bin/dir"
    ln -sf ls "$pkgdir/usr/bin/vdir"
    ln -sf touch "$pkgdir/usr/bin/mk"
    ln -sf id "$pkgdir/usr/bin/whoami"

}
