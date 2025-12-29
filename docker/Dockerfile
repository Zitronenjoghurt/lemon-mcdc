FROM lukemathwalker/cargo-chef:latest-rust-1.92 AS chef
WORKDIR /bot

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /bot/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian13 AS runtime
WORKDIR /bot
COPY --from=builder /bot/target/release/lemon-mcdc /usr/local/bin/bot
CMD ["/usr/local/bin/bot"]