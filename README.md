# `systemd-zram-setup@.service` generator for zram devices

This generator provides a simple and fast mechanism to configure swap on
`/dev/zram*` devices.

The main use case is create **swap** devices, but devices with a file
system can be created too, see below.

## Configuration

A default config file may be located in /usr. This generator checks the
following locations:

  - `/run/systemd/zram-generator.conf`
  - `/etc/systemd/zram-generator.conf`
  - `/usr/local/lib/systemd/zram-generator.conf`
  - `/usr/lib/systemd/zram-generator.conf`

... and the first file found in that list wins.

In addition, "drop-ins" will be loaded from `.conf` files in
`/etc/systemd/zram-generator.conf.d/`,
`/usr/lib/systemd/zram-generator.conf.d/`, etc.

The main configuration file is read before any of the drop-ins and has
the lowest precedence; entries in the drop-in files override entries in
the main configuration file.

See systemd.unit(5) for a detailed description of this logic.

See `zram-generator.conf.example` for a list of available settings.

## Swap devices

Create `/etc/systemd/zram-generator.conf`:

    # /etc/systemd/zram-generator.conf
    [zram0]
    zram-fraction = 0.5

A zram device will be created for each section. No actual configuration
is necessary (the default of `zram-fraction=0.5` will be used unless
overriden), but the configuration file with at least one section must
exist.

## Mount points

    # /etc/systemd/zram-generator.conf
    [zram1]
    mount-point = /var/tmp

This will set up a /dev/zram1 with ext2 and generate a mount unit for
/var/tmp.

## Rust

The second purpose of this program is to serve as an example of a
systemd generator in rust. Details are still being figured out.

## Installation

### Package Manager

It is recommended to use an existing package:

#### Fedora 33+

    sudo yum install -y zram-generator

#### Debian 11+ / Ubuntu 20.04+

    curl -skL https://keybase.io/nabijaczleweli/pgp_keys.asc | sudo apt-key add
    sudo apt-add-repository "deb https://debian.nabijaczleweli.xyz stable main"
    sudo apt install -y systemd-zram

#### Arch

    sudo pacman -S zram-generator

### Source

#### Fedora 33+ / CentOS 8+ / RHEL 8+

    sudo yum install -y cargo pkgconfig ruby-devel
    sudo gem install ronn-ng

#### Debian 10+ / Ubuntu 18.04+

    sudo apt update
    sudo apt install -y cargo pkg-config ruby-dev
    sudo gem install ronn-ng

#### openSUSE Leap 15.3+ / Tumbleweed

    sudo zypper install -y cargo pkgconfig ruby-devel
    sudo gem install ronn-ng
    sudo ln -s /usr/bin/ronn.* /usr/bin/ronn

#### Build and Install

Get the latest source code with:

    git clone https://github.com/systemd/zram-generator.git
    cd zram-generator

To install directly from sources, execute:

    make
    make test
    sudo make install

Which:

  - `zram-generator` binary is installed in the systemd system generator
    directory (usually `/usr/lib/systemd/system-generators/`)
  - `zram-generator(8)` and `zram-generator.conf(5)` manpages are
    installed into `/usr/share/man/manN/`, this requires
    [`ronn`](https://github.com/apjanke/ronn-ng).
  - `units/systemd-zram-setup@.service` is copied into the systemd
    system unit directory (usually `/usr/lib/systemd/system/`)
  - `zram-generator.conf.example` is copied into
    `/usr/share/doc/zram-generator/` You need though create your own
    config file at one of the locations listed above.

Ensure required kernel module load during system start:

    cat > /usr/lib/modules-load.d/zram-generator.conf <<-EOF
    zram
    EOF
    systemctl restart systemd-modules-load.service

Configure `zram0`, restart service and config zram device created:

    cat > /etc/systemd/zram-generator.conf <<-EOF
    [zram0]
    host-memory-limit = none
    max-zram-size = none
    zram-fraction = 1.0
    EOF
    systemctl restart systemd-zram-setup@zram0.service
    zramctl

After reboot your zram swap will be enabled automatically.

## Testing

The tests require either the `zram` module to be loaded, or root to run
`modprobe zram`.

Set the `ZRAM_GENERATOR_ROOT` environment variable to use that instead
of `/` as root.

The "{generator}" template in
`units/systemd-zram-setup@.service.d/binary-location.conf` can be
substituted for a non-standard location of the binary for testing.

## Authors

Written by:

  - Zbigniew Jędrzejewski-Szmek <zbyszek@in.waw.pl>
  - Igor Raits <i.gnatenko.brain@gmail.com>
  - наб <nabijaczleweli@gmail.com>
  - and others.

See <https://github.com/systemd/zram-generator/graphs/contributors> for
the full list.
