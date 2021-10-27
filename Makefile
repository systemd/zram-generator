# SPDX-License-Identifier: MIT

INSTALL = install
CARGO = cargo
CARGOFLAGS =
RONN = ronn
PKG_CONFIG = pkg-config
PREFIX = /usr

SYSTEMD_UTIL_DIR := $(shell $(PKG_CONFIG) --variable=systemdutildir systemd)
SYSTEMD_SYSTEM_UNIT_DIR := $(shell $(PKG_CONFIG) --variable=systemdsystemunitdir systemd)
SYSTEMD_SYSTEM_GENERATOR_DIR := $(shell $(PKG_CONFIG) --variable=systemdsystemgeneratordir systemd)
export SYSTEMD_UTIL_DIR

require_env = @[ -n "$($(1))" ] || { echo "\$$$(1) empty!" >&2; exit 1; }

.DEFAULT: build
.PHONY: build systemd-service program man check clean install

build: program systemd-service man

program:
	$(call require_env,SYSTEMD_UTIL_DIR)
	$(CARGO) build --release $(CARGOFLAGS)

systemd-service:
	$(call require_env,SYSTEMD_SYSTEM_GENERATOR_DIR)
	sed -e 's,@SYSTEMD_SYSTEM_GENERATOR_DIR@,$(SYSTEMD_SYSTEM_GENERATOR_DIR),' \
		< units/systemd-zram-setup@.service.in \
		> units/systemd-zram-setup@.service

man:
	$(RONN) --organization="zram-generator developers" man/*.md

check: program
	$(CARGO) test --release $(CARGOFLAGS)

clean:
	$(CARGO) clean
	rm -f units/systemd-zram-setup@.service

install: build
	$(call require_env,SYSTEMD_SYSTEM_GENERATOR_DIR)
	$(call require_env,SYSTEMD_SYSTEM_UNIT_DIR)
	$(call require_env,PREFIX)
	$(INSTALL) -Dpm755 target/release/zram-generator -t $(DESTDIR)$(SYSTEMD_SYSTEM_GENERATOR_DIR)/
	$(INSTALL) -Dpm644 units/systemd-zram-setup@.service -t $(DESTDIR)$(SYSTEMD_SYSTEM_UNIT_DIR)/
	$(INSTALL) -Dpm644 zram-generator.conf.example -t $(DESTDIR)$(PREFIX)/share/doc/zram-generator/
	$(INSTALL) -Dpm644 man/zram-generator.8 -t $(DESTDIR)$(PREFIX)/share/man/man8/
	$(INSTALL) -Dpm644 man/zram-generator.conf.5 -t $(DESTDIR)$(PREFIX)/share/man/man5/
