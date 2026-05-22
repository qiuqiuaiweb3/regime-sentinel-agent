FROM node:24-bookworm AS frontend
WORKDIR /app

COPY package.json package-lock.json ./
RUN npm ci

COPY svelte.config.js tsconfig.json vite.config.ts tailwind.config.cjs postcss.config.cjs ./
COPY src ./src
RUN npm run build

FROM rust:1.95-bookworm AS backend
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY apps ./apps
RUN cargo build --release -p regime-service --bin regime-service

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=backend /app/target/release/regime-service /usr/local/bin/regime-service
COPY --from=frontend /app/build /app/build

ENV HOST=0.0.0.0
ENV PORT=8080
ENV REGIME_STATIC_DIR=/app/build
ENV LIVE_COLLECTOR_ENABLED=false
ENV GEMINI_ENABLED=false

EXPOSE 8080
CMD ["regime-service"]
