set dotenv-load := true

# Show available recipes
default:
    @just --list

# Start local Postgres (and any other local services)
db-up:
    docker compose up -d postgres

# Stop local services
db-down:
    docker compose down

# Run pending migrations against the local database
db-migrate:
    sqlx migrate run --source migrations

# Run the app with hot reload (server + client)
dev:
    cargo leptos watch

# Format, lint, and test — the same gates CI runs
ci: fmt-check lint test

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# `web`'s ssr and hydrate halves are mutually exclusive features that each
# only compile for one target (native vs wasm32), so each needs its own pass.
lint:
    cargo clippy --workspace --all-targets --features web/ssr -- -D warnings
    cargo clippy -p web --no-default-features --features hydrate --target wasm32-unknown-unknown --lib -- -D warnings

test:
    cargo nextest run --workspace --features web/ssr

# Production-profile build (used by the Dockerfile)
build-release:
    cargo leptos build --release --precompress
