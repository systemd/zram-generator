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

### Testing

Set the `ZRAM_GENERATOR_ROOT` environment variable to use that
instead of `/` as root.
