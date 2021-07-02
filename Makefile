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

all: build man

.PHONY: build
build:
	@$(CARGO) build --release $(CARGOFLAGS)
	@sed -e 's,@SYSTEMD_SYSTEM_GENERATOR_DIR@,$(SYSTEMD_SYSTEM_GENERATOR_DIR),' \
		< units/systemd-zram-setup@.service.in \
		> units/systemd-zram-setup@.service

.PHONY: man
man:
	@$(RONN) --organization="zram-generator developers" man/*.md

.PHONY: test
test:
	@$(CARGO) test --release $(CARGOFLAGS)

.PHONY: install
install: install.bin install.man

install.bin:
	$(INSTALL) -Dpm755 target/release/zram-generator -t $(DESTDIR)$(SYSTEMD_SYSTEM_GENERATOR_DIR)/
	$(INSTALL) -Dpm644 units/systemd-zram-setup@.service -t $(DESTDIR)$(SYSTEMD_SYSTEM_UNIT_DIR)/
	$(INSTALL) -Dpm644 zram-generator.conf.example -t $(DESTDIR)$(PREFIX)/share/doc/zram-generator/

install.man:
	$(INSTALL) -Dpm644 man/zram-generator.8 -t $(DESTDIR)$(PREFIX)/share/man/man8/
	$(INSTALL) -Dpm644 man/zram-generator.conf.5 -t $(DESTDIR)$(PREFIX)/share/man/man5/

.PHONY: clean
clean:
	@$(CARGO) clean
	@rm -f units/systemd-zram-setup@.service
