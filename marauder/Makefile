.PHONY: debug

# rustup target add armv7-unknown-linux-musleabihf --toolchain=nightly
# cargo install cross

TARGET = armv7-unknown-linux-musleabihf
EXE = marauder
BIN = ./target/$(TARGET)/release/$(EXE)
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

whiteboard: DEVICE ?= root@10.11.99.1
whiteboard: EXE = whiteboard
whiteboard: BIN = ./target/$(TARGET)/release/$(EXE)
whiteboard: fmt
	cross build --release --frozen --locked --offline --target=$(TARGET) --bin $(EXE)
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	scp -pC $(BIN) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE) | tail -f'
