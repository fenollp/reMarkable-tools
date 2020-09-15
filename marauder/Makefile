.PHONY: debug

# rustup target add armv7-unknown-linux-musleabihf --toolchain=nightly
# cargo install cross

TARGET = armv7-unknown-linux-musleabihf
EXE = marauder
BIN = ./target/$(TARGET)/release/$(EXE)
IMG ?= rust-build-remarkable:latest
DEVICE ?= remarkable
RUN ?= docker run --rm --user $$(id -u):$$(id -g)
FLATC ?= $(RUN) -v "$$PWD"/src:/src -v "$$PWD"/src:/dst neomantra/flatbuffers:clang-v1.12.0-cc0.6.0 flatc

all: src/strokes/strokes_generated.rs lint
	cross build --release --target=$(TARGET)
	ls -lh $(BIN)

fmt:
	cargo fmt

lint: fmt
	cross clippy --target=$(TARGET)

test: fmt test-ujipenchars2
	cargo test

ujipenchars2.txt: url ?= https://archive.ics.uci.edu/ml/machine-learning-databases/uji-penchars/version2/ujipenchars2.txt
ujipenchars2.txt:
	curl -fSLo $@ $(url)
test-ujipenchars2: ujipenchars2.txt
	cargo run --bin ujipenchars $^

update:
	cargo update

debug: DEVICE ?= root@10.11.99.1
debug: all
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	scp -pC $(BIN) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE) | tail -f'

src/strokes/strokes_generated.rs: src/strokes/strokes.fbs
	$(FLATC) --rust -o /dst/strokes /$^

.PHONY: docker-env
docker-env:
	cd docker-toolchain && docker build \
		--build-arg UNAME=builder \
		--build-arg UID=$(shell id -u) \
		--build-arg GID=$(shell id -g) \
		--build-arg ostype=${shell uname} \
		--tag $(IMG) .

whiteboard: DEVICE ?= root@10.11.99.1
whiteboard: HOST ?= http://fknwkdacd.com:10000
whiteboard: EXE = whiteboard
whiteboard: BIN = ./target/armv7-unknown-linux-gnueabihf/release/$(EXE)
whiteboard: fmt docker-env
# 	cross build --release --bin $(EXE) --target=$(TARGET) --frozen --locked --offline
	docker volume inspect cargo-registry >/dev/null || docker volume create cargo-registry
	docker run \
		-t --rm \
		--user builder \
		-v $(shell pwd):/home/builder/libremarkable:rw \
		-v cargo-registry:/home/builder/.cargo/registry \
		-w /home/builder/libremarkable \
		$(IMG) \
		cargo build --release --bin $(EXE) --target=armv7-unknown-linux-gnueabihf #--locked #--frozen #--offline
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	scp -pC $(BIN) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE) --host=$(HOST) | tail -f'
