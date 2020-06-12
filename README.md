# `swap-create@.service` generator for zram devices

This program serves two purposes:

1. It is a simple and fast mechanism to configure `/dev/zram*` devices
   if the system has a small amount of memory.

   To create a zram device, create `/etc/systemd/zram-generator.conf`

   ```ini
   # /etc/systemd/zram-generator.conf
   [zram0]
   memory-limit = 2048
   zram-fraction = 0.25
   ```

   A zram device will be created for each section. No actual
   configuration is necessary (the defaults of 2048 and 0.25 will be
   used unless overriden), but the configuration file with at least
   one section must exist.

2. Once we figure out all the details, it should be useful as an
   example of a systemd generator in rust.

### Installation

Executing `make install` will create the following things:
* Generator binary installed as `/usr/lib/systemd/system-generators/zram-generator`
* `units/swap-create@.service` copied into `/usr/lib/systemd/system/`
* `zram-generator.conf.example` copied into `/usr/share/doc/zram-generator/`
You need though create your own config file at `/etc/systemd/zram-generator.conf`, customising it to your liking.

The "{generator}" template in `units/swap-create@.service.d/binary-location.conf`
can be substituted for a non-standard location of the binary for testing.

### Testing

Set the `ZRAM_GENERATOR_ROOT` environment variable to use that
instead of `/` as root.
