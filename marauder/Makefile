.PHONY: debug

# rustup target add armv7-unknown-linux-musleabihf --toolchain=nightly
# cargo install cross

EXE = marauder
TARGET = armv7-unknown-linux-musleabihf
DEVICE ?= remarkable

all: lint
	cross build --release --target=$(TARGET)

fmt:
	cargo fmt

lint: fmt

update:
	cargo update

debug: DEVICE ?= root@10.11.99.1
debug: all
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	scp ./target/$(TARGET)/release/$(EXE) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE)'
