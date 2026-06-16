# ── Stage 1: Build frontend ────────────────────────────────────────────
FROM node:22-slim AS frontend
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ .
RUN npm run build

# ── Stage 2: Build static binary (x86_64-musl or aarch64-musl) ───────
FROM rust:1.86-alpine AS builder
RUN apk add --no-cache musl-dev

WORKDIR /app

# Cache dependencies — copy only Cargo files first
COPY Cargo.toml Cargo.lock ./
COPY crates/trapfall-proto/Cargo.toml crates/trapfall-proto/Cargo.toml
COPY crates/trapfall-db/Cargo.toml crates/trapfall-db/Cargo.toml
COPY crates/trapfall-core/Cargo.toml crates/trapfall-core/Cargo.toml
COPY crates/trapfall-ingest/Cargo.toml crates/trapfall-ingest/Cargo.toml
COPY crates/trapfall-search/Cargo.toml crates/trapfall-search/Cargo.toml
COPY crates/trapfall-alert/Cargo.toml crates/trapfall-alert/Cargo.toml
COPY crates/trapfall-mcp/Cargo.toml crates/trapfall-mcp/Cargo.toml
COPY crates/trapfall-dashboard/Cargo.toml crates/trapfall-dashboard/Cargo.toml
COPY crates/trapfalld/Cargo.toml crates/trapfalld/Cargo.toml

# Create dummy source files for dependency caching
RUN mkdir -p crates/trapfall-proto/src && echo "" > crates/trapfall-proto/src/lib.rs && \
    mkdir -p crates/trapfall-db/src && echo "" > crates/trapfall-db/src/lib.rs && \
    mkdir -p crates/trapfall-core/src && echo "" > crates/trapfall-core/src/lib.rs && \
    mkdir -p crates/trapfall-ingest/src && echo "" > crates/trapfall-ingest/src/lib.rs && \
    mkdir -p crates/trapfall-search/src && echo "" > crates/trapfall-search/src/lib.rs && \
    mkdir -p crates/trapfall-alert/src && echo "" > crates/trapfall-alert/src/lib.rs && \
    mkdir -p crates/trapfall-mcp/src && echo "" > crates/trapfall-mcp/src/lib.rs && \
    mkdir -p crates/trapfall-dashboard/src && echo "" > crates/trapfall-dashboard/src/lib.rs && \
    mkdir -p crates/trapfalld/src && echo "fn main() {}" > crates/trapfalld/src/main.rs
RUN cargo build --release --features postgres --bin trapfall 2>/dev/null || true

# Copy real source and rebuild
COPY . .
COPY --from=frontend /app/web/build web/build
RUN touch crates/*/src/*.rs && cargo build --release --features postgres --bin trapfall

# ── Stage 3: Scratch runtime (zero OS overhead) ───────────────────────
FROM scratch

# Copy CA certs for HTTPS (reqwest needs this for webhook calls)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

WORKDIR /app
COPY --from=builder /app/target/release/trapfall /trapfall
COPY --from=frontend /app/web/build /app/web/build

ENV TRAPFALL_LISTEN=0.0.0.0:3000
ENV RUST_LOG=trapfall=info
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

VOLUME /data
EXPOSE 3000
ENTRYPOINT ["/trapfall"]
CMD ["serve"]
