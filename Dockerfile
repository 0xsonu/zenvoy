FROM rust:1.79-bookworm AS builder
WORKDIR /app
COPY src-tauri/ .
RUN cargo build --release --bin zenvoy-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/zenvoy-server /usr/local/bin/
ENV ZENVOY_VAULT_PATH=/data/vault
ENV ZENVOY_BIND=0.0.0.0:7878
EXPOSE 7878
VOLUME ["/data/vault"]
CMD ["zenvoy-server"]
