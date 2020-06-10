#!/bin/bash
# 
# A wrapper script for the zram-generator executable that conforms to the systemd
# generator interface documented at in the systemd.generator manual page, which does
# not support sub-commands acting as the generator.
zram-generator generate "$@"