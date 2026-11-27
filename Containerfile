# callboard の image。build の段で静的な binary を作り、動かす段は空の image に binary 1 つだけを置く。
# server は crates/server（package callboard・binary callboard-server）。image の中の path は /callboard-server。
# data は持たない（volume も付けない）。
# build の段の Rust の版は rust-toolchain.toml と合わせる（合わないと、build の中で別の版を取りに行く）。
FROM docker.io/library/rust:1.100.0-alpine@sha256:8d0e82abfdf9a38071e07cabc5afba4ddbe077631a80e4e742a760c38beadc33 AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p callboard

FROM scratch
COPY --from=build /src/target/release/callboard-server /callboard-server
# 聞く先（CALLBOARD_CASTLIST_URL・CALLBOARD_PAGETRAY_URL）は、動かす時に渡す（docs/src/run.md）。
ENV CALLBOARD_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/callboard-server"]
