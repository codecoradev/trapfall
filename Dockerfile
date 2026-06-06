# ── Stage 1: Build SvelteKit SPA ────────────────────────────────────────
FROM node:20-slim AS web-builder

WORKDIR /build/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web/ ./
RUN npm run build

# ── Stage 2: Build Rust binary ─────────────────────────────────────────
FROM rust:1.87-slim-bookworm AS rust-builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Copy SPA from stage 1
COPY --from=web-builder /build/web/build/ web/build/

RUN cargo build --release -p trapfalld

# ── Stage 3: Minimal runtime ───────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd -r trapfall && useradd -r -g trapfall -d /data trapfall

COPY --from=rust-builder /build/target/release/trapfalld /usr/local/bin/trapfall

# Data volume for SQLite
VOLUME /data

ENV RUST_LOG=info
ENV TRAPFALL_DB=/data/trapfall.db
ENV TRAPFALL_LISTEN=0.0.0.0:9090

EXPOSE 9090
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["trapfall", "healthcheck"]

USER trapfall
WORKDIR /data

ENTRYPOINT ["trapfall"]
CMD ["serve"]
