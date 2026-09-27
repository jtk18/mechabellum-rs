# Replay/stats server. Build from the repo root:
#   docker build -t mechabellum-server .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p mechabellum-server \
    && cp target/release/mechabellum-server /usr/local/bin/

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data mechabellum \
    && mkdir /data && chown mechabellum /data
COPY --from=build /usr/local/bin/mechabellum-server /usr/local/bin/
USER mechabellum
VOLUME /data
ENV MECHABELLUM_DB=/data/mechabellum.db \
    MECHABELLUM_BIND=0.0.0.0:8080 \
    RUST_LOG=info
EXPOSE 8080
ENTRYPOINT ["mechabellum-server"]
CMD ["serve"]
