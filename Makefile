MOONZIP_AUTHORITY=mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN
GLOBAL_ENV=eval '\
	export MOONZIP_AUTHORITY=$(MOONZIP_AUTHORITY) \
  '

ifneq (,$(filter n,$(MAKEFLAGS)))
GLOBAL_ENV=: GLOBAL_ENV
endif

.PHONY: prepare
prepare-env:
	avm use 0.30.1

.PHONY: build
build:
	cargo build --all

.PHONY: build-release
build-release:
	cargo build --release --all

.PHONY: build-program
build-program:
# Clean pumpfun-cpi to avoid build error. TODO: fix why build fails
	${GLOBAL_ENV} && \
		cargo clean -p pumpfun-cpi && \
		anchor build

.PHONY: unit-test
unit-test:
	${GLOBAL_ENV} && \
		cargo test

.PHONY: lint
lint:
	${GLOBAL_ENV} && \
		cargo fmt --check && \
		cargo clippy -- -D warnings
	yarn lint

.PHONY: db-migrate
db-migrate:
	sqlx migrate run --source backend/db/migrations

.PHONY: pre-commit
pre-commit:
	cargo sqlx prepare --workspace
	cargo run --bin api_gen -p backend
	yarn gen-api-client

.PHONY: test-env
test-env:
	DOCKER_BUILDKIT=1 docker build -t moonzip/dev:latest -f docker/Dockerfile.ci --build-arg MOONZIP_AUTHORITY=$(MOONZIP_AUTHORITY) --target dev .
	docker compose -f dev/docker-compose.dev.yml down -v --remove-orphans
	docker compose -f dev/docker-compose.dev.yml up -d --wait
	echo "DATABASE_URL=postgres://app-adm:app-adm-pass@localhost:15432/app-db?sslmode=disable" > .env
	echo "SQLX_OFFLINE=true" >> .env

.PHONY: e2e-test
e2e-test:
	docker run --net=host -t moonzip/dev:latest make e2e-test-exec

.PHONY: e2e-test-exec
e2e-test-exec:
	mkdir -p ./target
	[ -d ./idl ] && cp -r ./idl ./target/idl
	anchor run e2e-test

.PHONY: program-test
program-test:
	docker run --net=host -t moonzip/dev:latest make program-test-exec

.PHONY: program-test-exec
program-test-exec:
	mkdir -p ./target
	[ -d ./idl ] && cp -r ./idl ./target/idl
	anchor run program-test

.PHONY: test
ext-test: program-test e2e-test
