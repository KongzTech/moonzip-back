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
	${GLOBAL_ENV} && \
		cargo test && \
		anchor build && \
		anchor test

.PHONY: lint
lint:
	${GLOBAL_ENV} && \
		cargo fmt --check && \
		cargo clippy -- -D warnings
	yarn lint