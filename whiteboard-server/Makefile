all: lint

update:
	go get -u -a -v ./...
	go mod tidy
	go mod verify

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
