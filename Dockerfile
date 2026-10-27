# --- Builder ---------------------------------------------------------------
FROM rust:1 AS builder

# Toolchain/tool install is its own layer so it stays cached across deploys —
# it doesn't change anywhere near as often as the app source below does.
# mold matches .cargo/config.toml's linker override for Linux targets, which
# this image doesn't otherwise provide.
RUN apt-get update \
    && apt-get install -y --no-install-recommends mold \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-unknown-unknown \
    && cargo install cargo-leptos --locked

WORKDIR /app
COPY . .
# --precompress also disables cargo-leptos's default parallel front/server
# build, which otherwise interleaves output badly enough to obscure real
# build errors.
RUN RUST_BACKTRACE=full cargo leptos build --release --precompress

# --- Runtime -----------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home app

WORKDIR /app
COPY --from=builder /app/target/release/web /app/web
COPY --from=builder /app/target/site /app/site

ENV LEPTOS_SITE_ROOT=/app/site
ENV LEPTOS_SITE_ADDR=0.0.0.0:8080

USER app
EXPOSE 8080
CMD ["/app/web"]
