COMPOSE ?= docker-compose

all: lint

debug: lint
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --remove-orphans --force-recreate --build

lint:
	$(COMPOSE) config -q
