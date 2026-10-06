FROM rust:1.98-bookworm AS builder

RUN apt update && apt-get install -y pkg-config libjemalloc-dev libssl-dev && apt-get clean

WORKDIR /app

COPY . .

ENV RUST_BACKTRACE=1
ENV JEMALLOC_SYS_WITH_MALLOC_CONF="background_thread:true,tcache:false,dirty_decay_ms:100,muzzy_decay_ms:100,abort_conf:true"
RUN cargo build --release

FROM debian:bookworm AS base

RUN apt-get update && apt install -y libssl-dev ca-certificates libjemalloc-dev && apt-get clean

COPY --from=builder /app/target/release/robotlb /usr/local/bin/
ENV PATH=/usr/local/bin:$PATH
ENTRYPOINT ["/usr/local/bin/robotlb"]
