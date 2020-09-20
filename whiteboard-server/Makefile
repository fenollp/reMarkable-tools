EXE = whiteboard-server

GPB ?= 3.6.1
GPB_IMG ?= znly/protoc:0.4.0

COMPOSE ?= docker-compose
RUN ?= docker run --rm --user $$(id -u):$$(id -g)
PROTOC ?= $(RUN) -v "$$PWD/../:$$PWD/../" -w "$$PWD" $(GPB_IMG)
PROTOLOCK ?= $(RUN) -v $$PWD/../:/protolock -w /protolock nilslice/protolock  # commit --force

PB_FILES  = whiteboard.pb.go

all: lint
	CGO_ENABLED=0 go build -o $(EXE)$(if $(wildcard $(EXE)), || rm $(EXE))

debug:
	$(COMPOSE) config -q
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --force-recreate --build

update: SHELL := /bin/bash
update:
	[[ 'libprotoc $(GPB)' = $$($(PROTOC) --version) ]]
	go get -u -a -v ./...
	go mod tidy
	go mod verify
	$(COMPOSE) pull --ignore-pull-failures

whiteboard.pb.go: ../marauder/proto/hypercard/whiteboard.proto
	$(PROTOC) -I=.. --go_out=plugins=grpc:. $^
	mkdir -p hypercard_whiteboard && mv marauder/proto/hypercard/whiteboard.pb.go hypercard_whiteboard/ && rm -r marauder/proto/hypercard
	$(PROTOLOCK) commit

fmt: $(PB_FILES) $(X_FILES)
	go fmt ./...
vet:
	go vet ./...

lint: fmt vet lints

lints: SHELL = /bin/bash
lints:
	! diff -y <(git grep -lF 'ctx, cancel, err := srv.prepare(' -- hypercard_whiteboard/rpc_* | sort) <(ls -1 hypercard_whiteboard/rpc_* | sort) | grep '[<|>]' # Don't forget calling prepare()
	! git grep -InE 'Server.+uId' -- hypercard_whiteboard/rpc_* # rename uId to req
	! git grep -InEL 'rep = ' -- hypercard_whiteboard/rpc_* # RPCs MUST set rep at least once
	! git grep -InE ctxUID -- *.go | grep -vE 'func ctxUID|\(ctxUID|, ctxUID|== ctxUID|: +ctxUID' # use ctxUID(ctx) directly
	[[ $$(git grep -InEhc '^\s_ rabbiter = ') -eq $$(git grep -InEhc 'func.\([^()]+\) encodeBK\(\) \[\]string') ]] # assert all rabbiter impl.
	[[ $$(git grep -InEhc '^\s_ rabbiter = ') -eq $$(git grep -InEhc '^\s_ = verifyRabbiter\(') ]] # verify all rabbiter samples
