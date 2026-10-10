# syntax=docker/dockerfile:1
# Plain multi-stage build. Distroless and non-root are deferred (security.md SEC-12).

FROM rust:1.99.0-trixie AS build
# rdkafka builds librdkafka with cmake (docs/conventions/stack-architecture.md, Consequences).
# libcurl4-openssl-dev: librdkafka 2.12.1 includes <curl/curl.h> in a cmake build even though curl
# is off (upstream fixed it on master, after the 2.12.1 that rdkafka-sys 4.10 bundles). The base
# image may already have it; naming it keeps the build independent of that.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake libcurl4-openssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# No database during the image build: sqlx macros read the committed .sqlx/ query data.
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p bootstrap --bin app \
    && cp target/release/app /usr/local/bin/app

FROM debian:trixie-slim
WORKDIR /app
COPY --from=build /usr/local/bin/app /usr/local/bin/app
# `app db seed` reads seeds/<platform>/ relative to the working directory.
COPY seeds ./seeds
EXPOSE 8080 8081 8079
ENTRYPOINT ["app"]
CMD ["serve"]
