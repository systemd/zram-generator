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

.DEFAULT: build
.PHONY: build man check clean install

build: systemd_service
	@$(CARGO) build --release $(CARGOFLAGS)

systemd_service:
	@sed -e 's,@SYSTEMD_SYSTEM_GENERATOR_DIR@,$(SYSTEMD_SYSTEM_GENERATOR_DIR),' \
		< units/swap-create@.service.in \
		> units/swap-create@.service

man:
	@$(RONN) --organization="zram-generator developers" man/*.md

check: build
	@$(CARGO) test --release $(CARGOFLAGS)

clean:
	@$(CARGO) clean
	@rm -f units/swap-create@.service

install:
	$(INSTALL) -Dpm755 target/release/zram-generator $(DESTDIR)$(SYSTEMD_SYSTEM_GENERATOR_DIR)/zram-generator
	$(INSTALL) -Dpm644 units/swap-create@.service $(DESTDIR)$(SYSTEMD_SYSTEM_UNIT_DIR)/swap-create@.service
	$(INSTALL) -Dpm644 zram-generator.conf.example $(DESTDIR)$(PREFIX)/share/doc/zram-generator/zram-generator.conf.example
	$(INSTALL) -Dpm644 man/zram-generator.8 $(DESTDIR)$(PREFIX)/share/man/man8/zram-generator.8
	$(INSTALL) -Dpm644 man/zram-generator.conf.5 $(DESTDIR)$(PREFIX)/share/man/man5/zram-generator.conf.5
