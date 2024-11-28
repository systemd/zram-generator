# SPDX-License-Identifier: MIT
set -e

program=${1:?}

dir=$(mktemp -d 'zram-test.XXXXXX')
trap 'rm -r $dir' EXIT

set -x

$program --help
$program --version

# This should pass
mkdir $dir/{a,b,c}
$program $dir/{a,b,c}

# Those should fail with option parsing error (2)
set +e
$program dir1 dir2 dir3 dir4
ret=$?
set -e
test $ret -eq 2

set +e
$program dir1 --setup-device
ret=$?
set -e
test $ret -eq 2

# Those should fail because the device doesn't exist (1)
set +e
$program --setup-device /no/such/file/or/directory
ret=$?
set -e
test $ret -eq 1

set +e
$program --reset-device /no/such/file/or/directory
ret=$?
set -e
test $ret -eq 1
