FROM alpine:3.22 AS base

RUN apk add build-base gcc g++ make cmake libffi-dev openssl-dev openssl-libs-static libtool rustup

RUN rustup-init -y
ENV PATH="/root/.cargo/bin:$PATH"

COPY . .
RUN cargo build --release

FROM alpine:3.22 AS builder
COPY --from=base /target/release/one-mirror /app/one-mirror
WORKDIR /app

FROM scratch
COPY --from=builder / /
