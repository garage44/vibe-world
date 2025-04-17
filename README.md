# Vibers
A vibe-coding experiment to build an AR world on top of Rust & Bevy.

## Setup 
```bash
distrobox create -i archlinux:base --additional-flags "--volume=/tmp/.X11-unix:/tmp/.X11-unix:rw --volume=/run/user/$(id -u):/run/user/$(id -u):rw"
distrobox create -i archlinux:base --additional-flags "--volume=/tmp/.X11-unix:/tmp/.X11-unix:rw"
distrobox enter archlinux-base
sudo pacman -Syu
sudo pacman -S rust alsa-lib pkg-config libxcursor libxi libxkbcommon libxkbcommon-x11
export COOKIE=$(xauth list | grep "unix:0" | head -n1 | cut -d" " -f5)
xauth add :0 MIT-MAGIC-COOKIE-1 $COOKIE
cargo run
```