FROM rust:1.66-buster as builder

RUN cargo new --bin raxum
WORKDIR /raxum

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock

RUN cargo build --release

COPY . .
# this build step will cache your dependencies
RUN rm target/release/deps/rust*
RUN cargo build --release

CMD ["cargo", "run", "--release"]

# Prod stage
FROM gcr.io/distroless/cc

COPY --from=builder /raxum/target/release/rustio /bin/rustio

CMD ["rustio"]
