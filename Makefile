INSTALL = install
CARGO = cargo
RONN = ronn
PREFIX = /usr
SYSTEMD_DIR ?= $(PREFIX)/lib/systemd

.DEFAULT: build
.PHONY: build man check clean install

build:
	@$(CARGO) build --release

man:
	@$(RONN) --organization="zram-generator developers" man/*.md

check: build
	@$(CARGO) test --release

clean:
	@$(CARGO) clean

install: build man
	$(INSTALL) -Dpm755 target/release/zram-generator $(DESTDIR)$(SYSTEMD_DIR)/system-generators/zram-generator
	$(INSTALL) -Dpm644 units/swap-create@.service $(DESTDIR)$(SYSTEMD_DIR)/system/swap-create@.service
	$(INSTALL) -Dpm644 zram-generator.conf.example $(DESTDIR)$(PREFIX)/share/doc/zram-generator/zram-generator.conf.example
	$(INSTALL) -Dpm644 man/zram-generator.8 $(DESTDIR)$(PREFIX)/share/man/man8/zram-generator.8
	$(INSTALL) -Dpm644 man/zram-generator.conf.5 $(DESTDIR)$(PREFIX)/share/man/man5/zram-generator.conf.5
