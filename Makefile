MOONZIP_AUTHORITY=mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN
GLOBAL_ENV=eval '\
	export MOONZIP_AUTHORITY=$(MOONZIP_AUTHORITY) \
  '
PUSH_TO_ENV=eval '\
	rsync -e "docker exec -i" -Pavz --exclude-from=".dockerignore" --exclude-from=".gitignore" --exclude-from="$$HOME/.config/git/ignore" --filter "- .git/" . moonzip-dev-env:/app \
	'

ifneq (,$(filter n,$(MAKEFLAGS)))
GLOBAL_ENV=: GLOBAL_ENV
endif

.PHONY: prepare
prepare-env:
	avm use 0.30.1

.PHONY: build
build:
	anchor build

.PHONY: test
test:
# Clean pumpfun-cpi to avoid build error
	${GLOBAL_ENV} && \
		cargo test && \
		cargo clean -p pumpfun-cpi && \
		anchor build && \
		anchor test

.PHONY: lint
lint:
	${GLOBAL_ENV} && \
		cargo fmt --check && \
		cargo clippy -- -D warnings
	yarn lint

.PHONY: dev-env
dev-env:
	echo "DATABASE_URL=postgres://app-adm:app-adm-pass@localhost:15432/app-db?sslmode=disable" > .env
	echo "SQLX_OFFLINE=true" >> .env
	docker compose -f dev/docker-compose.dev.yml down -v && docker compose -f dev/docker-compose.dev.yml up -d --wait
	sqlx migrate run --source backend/db/migrations

.PHONY: enter-dev-env
enter-dev-env:
	DOCKER_BUILDKIT=1 docker build -t moonzip/ci:latest -f docker/Dockerfile.ci .
	DOCKER_BUILDKIT=1 docker build -t moonzip/dev:latest -f docker/Dockerfile.dev .

	docker rm -f moonzip-dev-env
	docker run --name=moonzip-dev-env -v rust-cache:/app/target -v node-cache:/app/node_modules --net=host -e MOONZIP_AUTHORITY=$(MOONZIP_AUTHORITY) -e DATABASE_URL=postgres://app-adm:app-adm-pass@localhost:15432/app-db?sslmode=disable -e SQLX_OFFLINE=true -t -d --rm moonzip/dev:latest /bin/bash
	${PUSH_TO_ENV}
	docker exec -it moonzip-dev-env /bin/bash

.PHONY: push-dev-env
push-dev-env:
	${PUSH_TO_ENV}

.PHONY: pre-commit
pre-commit:
	cargo sqlx prepare --workspace

