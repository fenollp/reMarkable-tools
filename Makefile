COMPOSE ?= docker-compose

all: lint

debug: lint
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --force-recreate --build

lint:
	$(COMPOSE) config -q
