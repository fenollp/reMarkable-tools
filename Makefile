COMPOSE ?= DOCKER_BUILDKIT=1 COMPOSE_DOCKER_CLI_BUILD=1 docker-compose

all: lint

debug: lint
	$(COMPOSE) rm -svf
	$(COMPOSE) up --abort-on-container-exit --remove-orphans --force-recreate --build

lint:
	$(COMPOSE) config -q

clean:
	$(COMPOSE) down
