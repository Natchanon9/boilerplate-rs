# Local development and the verification gate (docs/conventions/stack-architecture.md AC-3, AC-4).

SHELL := /bin/bash
.DEFAULT_GOAL := help

# SEC-8 (docs/conventions/security.md): SQL only through sqlx macros or bound parameters.
# sqlx refuses a non-'static SQL string unless it is wrapped in AssertSqlSafe, so dynamic
# SQL needs one of these escape hatches. Only the seed runner may use them.
SQL_ESCAPES := AssertSqlSafe|QueryBuilder|\.leak\(|Box::leak
SQL_ESCAPE_ALLOWED := crates/infrastructure/src/postgres/mod.rs
# SQL text built with format!, write!, or concat!.
SQL_FORMAT := (format|write|writeln|concat)!\s*\([^;]*"[^"]*\b(SELECT|INSERT|UPDATE|DELETE|WHERE|ORDER BY|LIMIT)\b

# Compose profiles equal the feature names in ENABLE_FEATURES in .env (docs/conventions/runtime.md),
# so `ENABLE_FEATURES=cache,kafka` activates the `cache` and `kafka` profiles and an empty value
# activates none. A lazy variable, so it reads .env after the `.env` target has created it.
# Exported, so `docker compose` in `up` and `logs` sees it.
export COMPOSE_PROFILES = $(shell sed -n 's/^ENABLE_FEATURES=//p' .env 2>/dev/null | tail -n 1 | tr -d ' \r')

.PHONY: help env up down logs migrate seed verify fmt clippy test deny sqlx-check sql-check ref-check check-skills

help: ## List the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-11s %s\n", $$1, $$2}'

env: .env ## Create .env from .env.example with a generated AUTH_JWT_SECRET

.env:
	@sed "s|^AUTH_JWT_SECRET=.*|AUTH_JWT_SECRET=$$(openssl rand -hex 32)|" .env.example > .env
	@echo "created .env with a random AUTH_JWT_SECRET"

up: .env ## Build and start the local stack (cache and kafka only if in ENABLE_FEATURES)
	@echo "features on: $(or $(COMPOSE_PROFILES),none)"
	@echo "starting: $$(docker compose config --services | tr '\n' ' ')"
	docker compose up -d --build

down: ## Stop the local stack, every profile
	docker compose --profile '*' down

logs: ## Follow the app logs
	docker compose logs -f app

migrate: ## Apply pending migrations to the local database
	cargo run -q -p bootstrap -- db migrate

seed: ## Load the demo seed data into the local database
	cargo run -q -p bootstrap -- db seed

verify: fmt clippy test deny sqlx-check sql-check ref-check check-skills ## The gate: every check must pass
	@echo "verify: all checks passed"

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --all-features

# Errors only: duplicate crate versions and unused licenses warn.
# Run `cargo deny check` for the full report.
deny:
	cargo deny --log-level error check

# Needs a live, migrated database (DATABASE_URL), so run `make up` first.
sqlx-check:
	cargo sqlx prepare --workspace --check -- --all-targets --all-features

sql-check:
	@if grep -rnE '$(SQL_ESCAPES)' crates --include='*.rs' | grep -v '^$(SQL_ESCAPE_ALLOWED):'; then \
		echo "sql-check: dynamic SQL escape hatch outside $(SQL_ESCAPE_ALLOWED) (security.md SEC-8)"; exit 1; fi
	@if grep -rnP '$(SQL_FORMAT)' crates --include='*.rs'; then \
		echo "sql-check: SQL built with a formatting macro (security.md SEC-8)"; exit 1; fi
	@echo "sql-check: ok"

ref-check: ## Check that doc references in code comments resolve
	@scripts/ref-check.sh

check-skills: ## Check the skills in .agents/skills against the Agent Skills spec
	@cargo xtask check-skills
