GLOBAL_ENV=eval '\
	export MOONZIP_AUTHORITY=mau6Cw3hZX7sPNtDcq69wNyyMbsNcUrubRmTPvtnkTN \
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
	echo "DATABASE_URL=postgres://app-adm:app-adm-pass@localhost:15432/app-db" > .env
	docker compose -f dev/docker-compose.dev.yml down -v && docker compose -f dev/docker-compose.dev.yml up -d
	sqlx migrate run --source backend/db/migrations

.PHONY: pre-commit
pre-commit:
	cargo sqlx prepare --workspace
