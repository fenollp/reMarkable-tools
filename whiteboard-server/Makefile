GPB ?= 3.6.1
GPB_IMG ?= znly/protoc:0.4.0

RUN ?= docker run --rm --user $$(id -u):$$(id -g)
PROTOC ?= $(RUN) -v "$$PWD/../:$$PWD/../" -w "$$PWD" $(GPB_IMG)
PROTOLOCK ?= $(RUN) -v $$PWD/../:/protolock -w /protolock nilslice/protolock  # commit --force

all: lint hypercards/whiteboard.pb.go

update: SHELL := /bin/bash
update:
	[[ 'libprotoc $(GPB)' = $$($(PROTOC) --version) ]]
	go get -u -a -v ./...
	go mod tidy
	go mod verify

hypercards/whiteboard.pb.go: ../marauder/proto/hypercard/whiteboard.proto
	$(PROTOC) -I=.. --go_out=plugins=grpc:. $^
	mv marauder/proto/hypercard/whiteboard.pb.go $@
	rmdir marauder/proto/hypercard marauder/proto marauder
	$(PROTOLOCK) commit

fmt:
	go fmt ./...
vet:
	go vet ./...

lint: fmt vet lints

lints: SHELL = /bin/bash
lints:
	! diff -y <(git grep -lF 'ctx, cancel, err := srv.prepare(' -- hypercards/rpc_* | sort) <(ls -1 hypercards/rpc_* | sort) | grep '[<|>]' # Don't forget calling prepare()
	! git grep -InE 'Server.+uId' -- hypercards/rpc_* # rename uId to req
	! git grep -InEL 'rep = ' -- hypercards/rpc_* # RPCs MUST set rep at least once
	! git grep -InE ctxUID -- *.go | grep -vE 'func ctxUID|\(ctxUID|, ctxUID|== ctxUID|: +ctxUID' # use ctxUID(ctx) directly
	[[ $$(git grep -InEhc '^\s_ rabbiter = ') -eq $$(git grep -InEhc 'func.\([^()]+\) encodeBK\(\) \[\]string') ]] # assert all rabbiter impl.
	[[ $$(git grep -InEhc '^\s_ rabbiter = ') -eq $$(git grep -InEhc '^\s_ = verifyRabbiter\(') ]] # verify all rabbiter samples
