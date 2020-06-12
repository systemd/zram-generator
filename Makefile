INSTALL_PROGRAM = install
CARGO_PROGRAM = cargo
PREFIX = /usr
SYSTEMD_DIR ?= $(PREFIX)/lib/systemd

.DEFAULT: build
.PHONY: build check clean install

build:
	@$(CARGO_PROGRAM) build --release

check: build
	@$(CARGO_PROGRAM) test --release

clean:
	@$(CARGO_PROGRAM) clean

install: build
	$(INSTALL_PROGRAM) -Dm755 target/release/zram-generator $(DESTDIR)$(SYSTEMD_DIR)/system-generators/zram-generator
	$(INSTALL_PROGRAM) -Dm644 units/swap-create@.service $(DESTDIR)$(SYSTEMD_DIR)/system/swap-create@.service
	$(INSTALL_PROGRAM) -Dm644 zram-generator.conf.example $(DESTDIR)$(PREFIX)/share/doc/zram-generator/zram-generator.conf.example