DEVICE ?= remarkable

# cargo install cross

TARGET ?= armv7-unknown-linux-musleabihf
LOCAL_TARGET = rustc -Vv | grep host: | cut -c7-

DOCKER ?= docker
COMPOSE ?= DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 $(DOCKER) compose
RUN ?= $(DOCKER) run --rm #--user $$(id -u):$$(id -g)

GPB ?= 3.6.1
PROTOC ?= $(RUN) -v "$(PWD):$(PWD)":rw -w "$(PWD)" znly/protoc:0.4.0
PROTOLOCK ?= $(RUN) -v $(PWD):/protolock:rw -w /protolock nilslice/protolock  # commit --force
FLATC ?= $(RUN) -v "$(PWD)"/src:/src -v "$(PWD)"/src:/dst neomantra/flatbuffers:clang-v1.12.0-cc0.6.0 flatc


all: lint


debug: lint whiteboard-server/hypercards/whiteboard.pb.go
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --remove-orphans --force-recreate --build

fmt:
	cargo +nightly fmt --all
	$(MAKE) $@ -C whiteboard-server

lint: fmt
	$(COMPOSE) config -q

clean:
	$(COMPOSE) down

update: SHELL := /bin/bash
update:
	[[ 'libprotoc $(GPB)' = $$($(PROTOC) --version) ]]
	cargo update
	$(MAKE) $@ -C whiteboard-server
	$(COMPOSE) pull --ignore-pull-failures
	$(COMPOSE) build http-server grpc-server


# marauder

shell:
	ssh -vvv $(DEVICE)

kill:
	ssh $(DEVICE) '/sbin/poweroff'

# ujipenchars2

marauder/ujipenchars2.txt: url ?= https://archive.ics.uci.edu/ml/machine-learning-databases/uji-penchars/version2/ujipenchars2.txt
marauder/ujipenchars2.txt:
	curl -fSLo $@ $(url)
test-ujipenchars2: marauder/ujipenchars2.txt
	cargo run --target=$$($(LOCAL_TARGET)) --bin ujipenchars $^

test: fmt test-ujipenchars2
	cargo test --target=$$($(LOCAL_TARGET))

# whiteboard

whiteboard-server/hypercards/whiteboard.pb.go: pb/proto/whiteboard.proto
	$(PROTOC) -I=. --go_out=plugins=grpc:. $^
	mv pb/proto/whiteboard.pb.go $@
	$(PROTOLOCK) commit

marauder/src/strokes/strokes_generated.rs: marauder/src/strokes/strokes.fbs
	$(FLATC) --rust -o /dst/strokes /$^

whiteboard: HOST ?= http://fknwkdacd.com:10000
whiteboard: WEBHOST ?= http://fknwkdacd.com:18888/s
whiteboard: PKG = marauder
whiteboard: EXE = whiteboard
whiteboard: marauder/src/strokes/strokes_generated.rs fmt
	cargo clippy                      --locked --frozen --offline                    --package=$(PKG) -- -D warnings --no-deps \
	  -W clippy::cast_lossless \
	  -W clippy::redundant_closure_for_method_calls \
	  -W clippy::str_to_string
	cross build --target-dir=target/x --locked --frozen --offline --target=$(TARGET) --package=$(PKG) --bin $(EXE) --release
	du -sh ./target/x/$(TARGET)/release/$(EXE)
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	rsync -a --stats --progress ./target/x/$(TARGET)/release/$(EXE) $(DEVICE):
	ssh $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug WHITEBOARD_WEBHOST=$(WEBHOST) ./$(EXE) --host=$(HOST) | tail -f'


# scrolls

#tmux a -t rM-scrolls || tmux new -s rM-scrolls
#tmux send-keys -t rM-scrolls:0 'echo y' Enter

scrolls: EXE = scrolls
scrolls: SES = rM-$(EXE)
scrolls: fmt
	cargo clippy                      --locked --frozen --offline                    --package=$(EXE) -- -D warnings --no-deps \
	  -W clippy::cast_lossless \
	  -W clippy::redundant_closure_for_method_calls \
	  -W clippy::str_to_string
	cross build --target-dir=target/x --locked --frozen --offline --target=$(TARGET) --package=$(EXE) --bin $(EXE) --release
	du -sh ./target/x/$(TARGET)/release/$(EXE)
	ssh $(DEVICE) 'killall -q -9 $(EXE) || true; systemctl stop xochitl || true'
	rsync -a --stats --progress ./target/x/$(TARGET)/release/$(EXE) $$(find . -maxdepth 1 -type f \( -iname \*.jsonl -o -iname \*.ndjson \) -printf '%T@\t%p\n' | sort -nr | cut -f2-) $(DEVICE):
	ssh -t $(DEVICE) 'RUST_BACKTRACE=1 RUST_LOG=debug ./$(EXE) $$(find . -maxdepth 1 -type f \( -iname \*.jsonl -o -iname \*.ndjson \))'
