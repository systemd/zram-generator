<!-- SPDX-License-Identifier: MIT -->

zram-generator.conf(5) -- Systemd unit generator for zram swap devices (configuration)
======================================================================================

## SYNOPSIS

`/usr/lib/systemd/zram-generator.conf`<br />
`/usr/local/lib/systemd/zram-generator.conf`<br />
`/etc/systemd/zram-generator.conf`<br />
`/run/systemd/zram-generator.conf`

`/usr/lib/systemd/zram-generator.conf.d/*.conf`<br />
`/usr/local/lib/systemd/zram-generator.conf.d/*.conf`<br />
`/etc/systemd/zram-generator.conf.d/*.conf`<br />
`/run/systemd/zram-generator.conf.d/*.conf`

## DESCRIPTION

These files configure devices created by zram-generator(8). See systemd.syntax(5) for a general description of the syntax.

## CONFIGURATION DIRECTORIES AND PRECEDENCE

The default configuration doesn't specify any devices. Consult */usr/share/zram-generator/zram-generator.conf.example* for an example configuration file.

When packages need to customize the configuration, they can install configuration snippets in */usr/lib/systemd/zram-generator.conf.d/*.
Files in */etc/* are reserved for the local administrator, who may use this logic to override the configuration files installed by vendor packages.
The main configuration file is read before any of the configuration directories, and has the lowest precedence;
entries in a file in any configuration directory override entries in the single configuration file.
Files in the *\*.conf.d/* configuration subdirectories are sorted by their filename in lexicographic order, regardless of which of the subdirectories they reside in.
When multiple files specify the same option, for options which accept just a single value, the entry in the file with the lexicographically latest name takes precedence.
It is recommended to prefix all filenames in those subdirectories with a two-digit number and a dash, to simplify the ordering of the files.

To disable a configuration file supplied by the vendor, the recommended way is to place a symlink to */dev/null* in the configuration directory in */etc/*,
with the same filename as the vendor configuration file.

The generator understands the following option on the kernel command-line: `systemd.zram[=0|1]`.
When specified with a true argument (or no argument), the `zram0` device will be created.
Default options apply, but may be overridden by configuration on disk if present.
When specified with a false argument, no zram devices will be created by the generator.
This option thus has higher priority than the configuration files.

## OPTIONS

Each device is configured independently in its `[zramN]` section, where N is a nonnegative integer. Other sections are ignored.

Devices with the final size of *0* will be discarded.

* `host-memory-limit`=

  Sets the upper limit on the total usable RAM (as defined by *MemTotal* in `/proc/meminfo`, confer proc(5)) above which the device will *not* be created.

  This takes a nonnegative number, representing that limit in megabytes, or the literal string *none*, which can be used to override a limit set earlier.

  Defaults to *none*.

* `zram-size`=

  Sets the size of the zram device as a function of *MemTotal*, available as the `ram` variable.

  Arithmetic operators (^%/\*-+), e, π, SI suffixes, log(), int(), ceil(), floor(), round(), abs(), min(), max(), and trigonometric functions are supported.

  Defaults to *min(ram / 2, 4096)*.

* `compression-algorithm`=

  Specifies the algorithm used to compress the zram device.

  This takes a literal string, representing the algorithm to use.<br />
  Consult */sys/block/zram0/comp_algorithm* for a list of currently loaded compression algorithms, but note that additional ones may be loaded on demand.

  If unset, none will be configured and the kernel's default will be used.

* `writeback-device`=

  Write incompressible pages, for which no gain was achieved, to the specified device under memory pressure.
  This corresponds to the */sys/block/zramX/backing_dev* parameter.

  Takes a path to a block device, like */dev/disk/by-partuuid/2d54ffa0-01* or */dev/zvol/tarta-zoot/swap-writeback*.

  If unset, none is used, and incompressible pages are kept in RAM.

* `swap-priority`=

  Controls the relative swap priority, a value between -1 and 32767. Higher numbers indicate higher priority.

  If unset, 100 is used.

* `mount-point`=

  Format the device with a file system (not as swap) and mount this file system over the specified directory.
  When neither this option nor `fs-type`= is specified, the device will be formatted as swap.

  Note that the device is temporary: contents will be destroyed automatically after the file system is unmounted (to release the backing memory).

* `fs-type`=

  Specifies how the device shall be formatted. The default is *ext2* if `mount-point` is specified, and *swap* otherwise. (Effectively, the device will be formatted as swap, if neither `fs-type`= nor `mount-point`= are specified.)

  Note that the device is temporary: contents will be destroyed automatically after the file system is unmounted (to release the backing memory).

  Also see systemd-makefs(8).

* `options`=

  Sets mount or swapon options. Availability depends on `fs-type`.

  Defaults to *discard*.

## ENVIRONMENT VARIABLES

Setting `ZRAM_GENERATOR_ROOT` during parsing will cause */proc/meminfo* to be read from *$ZRAM_GENERATOR_ROOT/proc/meminfo* instead,
and *{/usr/lib,/usr/local/lib,/etc,/run}/systemd/zram-generator.conf* to be read from *$ZRAM_GENERATOR_ROOT/{/usr/lib,/usr/local/lib,/etc,/run}/systemd/zram-generator.conf*.

## EXAMPLES

The default configuration will yield the following:

     zram device size
         ^
         │
      4G>│               ooooooooooooo
         │             o
         │           o
         │         o
      2G>│       o
         │     o
         │   o
    512M>│ o
         0───────────────────────> total usable RAM
           ^     ^       ^
           1G    4G      8G

A piecewise-linear size 1:1 for the first 4G, then 1:2 above, up to a max of 32G:<br />
&nbsp;&nbsp;`zram-size = min(min(ram, 4096) + max(ram - 4096, 0) / 2, 32 * 1024)`

     zram device size
         ^
     32G>|                                                oooooooooooooo
         |                                            o
     30G>|                                        o
         |
        /=/
         |
      8G>│                           o
         │                       o
         │                   o
         │               o
         │           o
      4G>│       o
         │     o
         │   o
      1G>│ o
         0───────────────────────────────────||──────────────────────> total usable RAM
           ^     ^       ^               ^        ^       ^       ^
           1G    4G      8G             12G      56G     60G     64G



## OBSOLETE OPTIONS

* `memory-limit`=

  Compatibility alias for `host-memory-limit`.

* `zram-fraction`=

  Defines the scaling factor of the zram device's size with relation to the total usable RAM.

  This takes a nonnegative floating-point number representing that factor.

  Defaulted to *0.5*. Setting this or `max-zram-size` overrides `zram-size`.

* `max-zram-size`=

  Sets the limit on the zram device's size obtained by `zram-fraction`.

  This takes a nonnegative number, representing that limit in megabytes, or the literal string *none*, which can be used to override a limit set earlier.

  Defaulted to *4096*. Setting this or `zram-fraction` overrides `zram-size`.

## REPORTING BUGS

<https://github.com/systemd/zram-generator/issues>

## SEE ALSO

zram-generator(8), systemd.syntax(5), proc(5)

<https://github.com/systemd/zram-generator>

Linux documentation of zram: <https://kernel.org/doc/html/latest/admin-guide/blockdev/zram.html><br />
     and the zram sysfs ABI: <https://kernel.org/doc/Documentation/ABI/testing/sysfs-block-zram>

`fasteval` documentation for the entire `zram-size` arithmetic DSL: <https://docs.rs/fasteval/0.2.4/fasteval/#the-fasteval-expression-mini-language>
