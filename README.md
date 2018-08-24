# swap-create@.service generator for zram devices

This program serves two purposes:

1. It is a simple and fast mechanism to configure /dev/zram0 devices if
   the system has a small amount of memory.

   By default the unit will be created if the system has less than 2GB
   of RAM. The zram device will use 25% of it.

   Configuration file can be used to override those defaults:
   # /etc/systemd/zram-generator.conf
   [zram0]
   memory-limit = 4096
   zram-fraction = 0.10

   If the configuration file exists, it overrides the defaults. Each
   section in the configuration file is turned into one zram device,
   memory limits permitting. If the configuration file is empty, no
   devices are created.

2. Once we figure out all the detail, it should be useful as an
   example of a systemd generator in rust.
