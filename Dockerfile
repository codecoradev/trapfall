# ── Stage 1: Prepare recipe (dependency fingerprint only) ──────────────
FROM rust:1.86-slim-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

# ── Stage 2: Analyze dependencies (creates recipe.json) ───────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build dependencies (cached layer) ────────────────────────
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source and build application
COPY . .
RUN cargo build --release

# ── Stage 4: Build frontend ───────────────────────────────────────────
FROM node:20-slim AS frontend
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web/ .
RUN npm run build

# ── Stage 5: Minimal runtime ─────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/trapfall /usr/local/bin/trapfall
COPY --from=frontend /app/web/build /app/web/build

ENV TRAPFALL_LISTEN=0.0.0.0:3000
ENV RUST_LOG=trapfall=info

EXPOSE 3000
ENTRYPOINT ["trapfall"]
CMD ["serve"]
