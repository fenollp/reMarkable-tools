COMPOSE ?= DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 docker compose

# cargo install cross

TARGET ?= armv7-unknown-linux-musleabihf
LOCAL_TARGET = rustc -Vv | grep host: | cut -c7-

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

shell:
	ssh -vvv $(DEVICE)

kill:
	ssh $(DEVICE) '/sbin/poweroff'

marauder/src/strokes/strokes_generated.rs: marauder/src/strokes/strokes.fbs
	$(FLATC) --rust -o /dst/strokes /$^

whiteboard: HOST ?= http://fknwkdacd.com:10000
whiteboard: WEBHOST ?= http://fknwkdacd.com:18888/s
whiteboard: EXE = whiteboard
whiteboard: BIN = ./target/$(TARGET)/release/$(EXE)
whiteboard: marauder/src/strokes/strokes_generated.rs fmt
	cross clippy --package=marauder --target=$(TARGET) -- -W clippy::pedantic
	cross build --package=marauder --target=$(TARGET) --release --bin $(EXE) --locked --frozen --offline
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	rsync -a --stats --progress $(BIN) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug WHITEBOARD_WEBHOST=$(WEBHOST) ./$(EXE) --host=$(HOST) | tail -f'


# scrolls

#tmux a -t rM-scrolls || tmux new -s rM-scrolls
#tmux send-keys -t rM-scrolls:0 'echo y' Enter

scrolls: EXE = scrolls
scrolls: BIN = ./target/$(TARGET)/release/$(EXE)
scrolls: SES = rM-$(EXE)
scrolls: fmt
	cross clippy --locked --frozen --offline --all-features --target=$(TARGET) --package=$(EXE) -- -D warnings --no-deps \
	  -W clippy::cast_lossless -W clippy::redundant_closure_for_method_calls -W clippy::str_to_string
	cross build  --locked --frozen --offline --all-features --target=$(TARGET) --package=$(EXE) --bin $(EXE)  --release
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	rsync -a --stats --progress $(BIN) "$$(ls -t ./*.jsonl | head -n1)" $(DEVICE):
	ssh -t $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE) '"$$(ls -t ./*.jsonl | head -n1)"
