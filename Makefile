COMPOSE ?= DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 docker compose

# cargo install cross

TARGET ?= armv7-unknown-linux-musleabihf
LOCAL_TARGET = rustc -Vv | grep host: | cut -c7-

EXE = marauder
BIN = ./target/$(TARGET)/release/$(EXE)

DEVICE ?= remarkable

RUN ?= docker run --rm --user $$(id -u):$$(id -g)
FLATC ?= $(RUN) -v "$$PWD"/src:/src -v "$$PWD"/src:/dst neomantra/flatbuffers:clang-v1.12.0-cc0.6.0 flatc


all: lint


debug: lint
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --remove-orphans --force-recreate --build


fmt:


lint: fmt
	$(COMPOSE) config -q


marauder/ujipenchars2.txt: url ?= https://archive.ics.uci.edu/ml/machine-learning-databases/uji-penchars/version2/ujipenchars2.txt
marauder/ujipenchars2.txt:
	curl -fSLo $@ $(url)
test-ujipenchars2: marauder/ujipenchars2.txt
	cargo run --target=$$($(LOCAL_TARGET)) --bin ujipenchars $^

test: fmt test-ujipenchars2
	cargo test --target=$$($(LOCAL_TARGET))


clean:
	$(COMPOSE) down


update:
	cargo update
	$(MAKE) $@ -C whiteboard-server
	$(COMPOSE) pull --ignore-pull-failures
	$(COMPOSE) build http-server grpc-server


# marauder

kill:
	ssh $(DEVICE) '/sbin/poweroff'

marauder/src/strokes/strokes_generated.rs: marauder/src/strokes/strokes.fbs
	$(FLATC) --rust -o /dst/strokes /$^

whiteboard: HOST ?= http://fknwkdacd.com:10000
whiteboard: WEBHOST ?= http://fknwkdacd.com:18888/s
whiteboard: EXE = whiteboard
whiteboard: marauder/src/strokes/strokes_generated.rs fmt
	cross clippy --target=$(TARGET) -- -W clippy::pedantic
	cross build --target=$(TARGET) --release --bin $(EXE) --locked --frozen --offline
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	rsync -a --stats --progress $(BIN) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug WHITEBOARD_WEBHOST=$(WEBHOST) ./$(EXE) --host=$(HOST) | tail -f'
