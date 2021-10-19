<!-- SPDX-License-Identifier: MIT -->

zram-generator(8) -- Systemd unit generator for zram swap devices
=================================================================

## SYNOPSIS

`/usr/lib/systemd/system-generators/zram-generator` `TARGET_DIR` [*2RGET_DIR* *3RGET_DIR*]<br />
`/usr/lib/systemd/system-generators/zram-generator` --setup-device `DEVICE`<br />
`/usr/lib/systemd/system-generators/zram-generator` --reset-device `DEVICE`

## DESCRIPTION

`zram-generator` is a generator that creates systemd units to format and use compressed RAM devices, either as swap or a file system.


The generator will be invoked by systemd early at boot. The generator will then:

  1. read configuration files from *{/etc,/lib}/systemd/zram-generator.conf[.d]* (see zram-generator.conf(5) for details);
  2. generate systemd.swap(5) and/or systemd.mount(5) units into `TARGET_DIR` and connect them to `swap.target` or `local-fs.target` as appropriate;
  3. ensure the `zram` module is loaded and create the requested devices.

The generator does nothing if run inside a container (as determined by *systemd-detect-virt(8) --container*).

The generator also understands the kernel command-line option `systemd.zram`. See zram-generator.conf(5) for details.

Setting the `ZRAM_GENERATOR_ROOT` environment variable makes the generator run in test mode, in which case containerisation is ignored and step `3` is skipped.<br />
For the ramifications of `ZRAM_GENERATOR_ROOT` on config handling, see zram-generator.conf(5).


Generated *dev-zramN.swap* units depend on `systemd-zram-setup@zramN.service`, which will:

  1. read configuration files from *{/etc,/lib}/systemd/zram-generator.conf[.d]* (see zram-generator.conf(5) for details);
  2. set the desired compression algorithm, if any;
     if the current kernel doesn't understand the specified algorithm, a warning is issued, but execution continues;
  3. set the desired blockdev size and format it as swap with *systemd-makefs(8)*.

Generated *path-to-mount-point.mount* units depend on `systemd-zram-setup@zramN.service`.
The effect is similar to what happens for swap units, but of course they are formatted with a file system.

When the unit is stopped, the zram device is reset, freeing memory and allowing the device to be reused.

`zram-generator` implements systemd.generator(7).

### Applying config changes

This generator is invoked in early boot, and the devices it configures will be created very early too,
so the easiest way to apply config changes is to simply reboot the machine.

Nevertheless, sometimes it may be useful to add new devices or apply config changes at runtime.
Applying new configuration means restarting the units, and that in turn means recreating the zram devices.
This means that *file systems are temporarily unmounted and their contents lost*, and *pages are moved out of the compressed swap device* into other memory.
If this is acceptable, `systemctl restart systemd-zram-setup@zramN` or `systemctl restart systemd-zram-setup@*`
may be used to recreate a specific device or all configured devices.
(If the device didn't exist, `restart` will create it.)
If the way the device is used (e.g. the mount point or file system type) is changed,
`systemctl daemon-reload` needs to be called first to recreate systemd units.
If a device or mount point is removed from configuration, the unit should be stopped before calling `daemon-reload`.
Otherwise, systemd will not know how to stop the unit properly.

## REPORTING BUGS

<https://github.com/systemd/zram-generator/issues>

## SEE ALSO

zram-generator.conf(5), systemd.generator(7), systemd.swap(5)

<https://github.com/systemd/zram-generator>

Linux documentation of zram: <https://kernel.org/doc/html/latest/admin-guide/blockdev/zram.html><br />
     and the zram sysfs ABI: <https://kernel.org/doc/Documentation/ABI/testing/sysfs-block-zram>
