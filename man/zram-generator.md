
[comment]: SPDX-License-Identifier: MIT

zram-generator(8) -- Systemd unit generator for zram swap devices
=================================================================

## SYNOPSIS

`/usr/lib/systemd/system-generators/zram-generator` `TARGET_DIR` [*2RGET_DIR* *3RGET_DIR*]<br />
`/usr/lib/systemd/system-generators/zram-generator` --setup-device `DEVICE`<br />
`/usr/lib/systemd/system-generators/zram-generator` --reset-device `DEVICE`

## DESCRIPTION

`zram-generator` is a generator that creates systemd units to format and use compressed RAM devices, either as swap or a mount point.


The generator will be invoked by systemd early at boot. The generator will then:

  1. read configuration files from *{/etc,/lib}/systemd/zram-generator.conf[.d]* (see zram-generator.conf(5) for details);
  2. generate systemd.swap(5) and/or systemd.mount(5) units into `TARGET_DIR` and connect them to `swap.target` or `local-fs.target` as appropriate;
  3. ensure the `zram` module is loaded and create the requested devices.

The generator does nothing if run inside a container (as determined by *systemd-detect-virt(8) --container*).

The generator also understands the kernel command-line option `systemd.zram`. See zram-generator.conf(5) for details.

Setting the `ZRAM_GENERATOR_ROOT` environment variable makes the generator run in test mode, in which case containerisation is ignored and step `3` is skipped.<br />
For the ramifications of `ZRAM_GENERATOR_ROOT` on config handling, see zram-generator.conf(5).


Generated *dev-zramN.swap* units depend on `systemd-swap-create@zramN.service`, which will:

  1. read configuration files from *{/etc,/lib}/systemd/zram-generator.conf[.d]* (see zram-generator.conf(5) for details);
  2. set the desired compression algorithm, if any;
     if the current kernel doesn't understand the specified algorithm, a warning is issued, but execution continues;
  3. set the desired blockdev size and format it as swap with *systemd-makefs(8)*.

Generated *path-to-mount-point.mount* units depend on `systemd-swap-create@zramN.service`.
The effect is similar to what happens for swap units, but of course they are formatted with a file system.

When the unit is stopped, the zram device is reset, freeing memory and allowing the device to be reused.


`zram-generator` implements systemd.generator(7).

## REPORTING BUGS

&lt;<https://github.com/systemd/zram-generator/issues>&gt;

## SEE ALSO

zram-generator.conf(5), systemd.generator(7), systemd.swap(5)

&lt;<https://github.com/systemd/zram-generator>&gt;

Linux documentation of zram: &lt;<https://kernel.org/doc/html/latest/admin-guide/blockdev/zram.html>&gt;<br />
     and the zram sysfs ABI: &lt;<https://kernel.org/doc/Documentation/ABI/testing/sysfs-block-zram>&gt;
