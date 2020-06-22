# `swap-create@.service` generator for zram devices

This generator provides a simple and fast mechanism to configure swap on `/dev/zram*` devices.

Create `/etc/systemd/zram-generator.conf`:

```ini
# /etc/systemd/zram-generator.conf
[zram0]
zram-fraction = 0.5
```

A zram device will be created for each section. No actual
configuration is necessary (the default of zram-fraction=0.5 will be
used unless overriden), but the configuration file with at least one
section must exist.

A default config file may be located in /usr.
This generator checks the following locations:
* `/run/systemd/zram-generator.conf`
* `/etc/systemd/zram-generator.conf`
* `/usr/local/lib/systemd/zram-generator.conf`
* `/usr/lib/systemd/zram-generator.conf`

… and the first file found in that list wins.

In addition, "drop-ins" will be loaded from `.conf` files in
`/etc/systemd/zram-generator.conf.d/`,
`/usr/lib/systemd/zram-generator.conf.d/`, etc.

The main configuration file is read before any of the drop-ins and has
the lowest precedence; entries in the drop-in files override entries
in the main configuration file.

See systemd.unit(5) for a detailed description of this logic.

See `zram-generator.conf.example` for a list of available settings.


The second purpose of this program is to serve as an example of a
systemd generator in rust. Details are still being figured out.

### Installation

Executing `make install` will create the following things:
* Generator binary installed as `/usr/lib/systemd/system-generators/zram-generator`
* `zram-generator(8)` and `zram-generator.conf(5)` manpages installed into `/usr/share/man/manN/`, this requires [`ronn`](https://github.com/apjanke/ronn-ng).
* `units/swap-create@.service` copied into `/usr/lib/systemd/system/`
* `zram-generator.conf.example` copied into `/usr/share/doc/zram-generator/`
You need though create your own config file at one of the locations listed above.

### Testing

The tests require either the `zram` module to be loaded, or root to run `modprobe zram`.

Set the `ZRAM_GENERATOR_ROOT` environment variable to use that
instead of `/` as root.

The "{generator}" template in `units/swap-create@.service.d/binary-location.conf`
can be substituted for a non-standard location of the binary for testing.
