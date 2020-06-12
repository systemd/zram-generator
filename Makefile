INSTALL = install
CARGO = cargo
PREFIX = /usr
SYSTEMD_DIR ?= $(PREFIX)/lib/systemd

.DEFAULT: build
.PHONY: build check clean install

build:
	@$(CARGO) build --release

check: build
	@$(CARGO) test --release

clean:
	@$(CARGO) clean

install: build
	$(INSTALL) -Dpm755 target/release/zram-generator $(DESTDIR)$(SYSTEMD_DIR)/system-generators/zram-generator
	$(INSTALL) -Dpm644 units/swap-create@.service $(DESTDIR)$(SYSTEMD_DIR)/system/swap-create@.service
	$(INSTALL) -Dpm644 zram-generator.conf.example $(DESTDIR)$(PREFIX)/share/doc/zram-generator/zram-generator.conf.example
