
[comment]: SPDX-License-Identifier: MIT

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

  For compatibility with earlier versions, `memory-limit` is allowed as an alias for this option.
  Its use is discouraged, and administrators should migrate to `host-memory-limit`.

* `zram-fraction`=

  Defines the scaling factor of the zram device's size with relation to the total usable RAM.

  This takes a nonnegative floating-point number representing that factor.

  Defaults to *0.5*.

* `max-zram-size`=

  Sets the limit on the zram device's size obtained by `zram-fraction`.

  This takes a nonnegative number, representing that limit in megabytes, or the literal string *none*, which can be used to override a limit set earlier.

  Defaults to *4096*.

* `compression-algorithm`=

  Specifies the algorithm used to compress the zram device.

  This takes a literal string, representing the algorithm to use.<br />
  Consult */sys/block/zram0/comp_algorithm* for a list of currently loaded compression algorithms, but note that additional ones may be loaded on demand.

  If unset, none will be configured and the kernel's default will be used.

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

## ENVIRONMENT VARIABLES

Setting `ZRAM_GENERATOR_ROOT` during parsing will cause */proc/meminfo* to be read from *$ZRAM_GENERATOR_ROOT/proc/meminfo* instead,
and *{/usr/lib,/usr/local/lib,/etc,/run}/systemd/zram-generator.conf* to be read from *$ZRAM_GENERATOR_ROOT/{/usr/lib,/usr/local/lib,/etc,/run}/systemd/zram-generator.conf*.

## EXAMPLES

The default configuration will yield the following:

     zram device size [MB]
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
         0───────────────────────> total usable RAM [MB]
           ^     ^       ^
           1G    4G      8G

## REPORTING BUGS

&lt;<https://github.com/systemd/zram-generator/issues>&gt;

## SEE ALSO

zram-generator(8), systemd.syntax(5), proc(5)

&lt;<https://github.com/systemd/zram-generator>&gt;

Linux documentation of zram: &lt;<https://kernel.org/doc/html/latest/admin-guide/blockdev/zram.html>&gt;<br />
     and the zram sysfs ABI: &lt;<https://kernel.org/doc/Documentation/ABI/testing/sysfs-block-zram>&gt;
