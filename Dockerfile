# syntax=docker.io/docker/dockerfile:1.7-labs

FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

LABEL org.opencontainers.image.source=https://github.com/reamlabs/ream
LABEL org.opencontainers.image.description="Ream is a modular, open-source Ethereum beam chain client."
LABEL org.opencontainers.image.licenses="MIT"

# Install system dependencies
RUN apt-get update && apt-get -y upgrade && apt-get install -y libclang-dev pkg-config

# Builds a cargo-chef plan
FROM chef AS planner
COPY --exclude=.git --exclude=dist . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build profile, release by default
ARG BUILD_PROFILE=release
ENV BUILD_PROFILE=$BUILD_PROFILE

# Extra Cargo flags
ARG RUSTFLAGS=""
ENV RUSTFLAGS="$RUSTFLAGS"

# Extra Cargo features
ARG FEATURES=""
ENV FEATURES=$FEATURES

# Build dependencies
RUN cargo chef cook --profile $BUILD_PROFILE --features "$FEATURES" --recipe-path recipe.json

# Build application
COPY --exclude=.git --exclude=dist . .
RUN cargo build --profile $BUILD_PROFILE --features "$FEATURES" --locked --bin ream

# ARG is not resolved in COPY so we have to hack around it by copying the
# binary to a temporary location
RUN cp /app/target/$BUILD_PROFILE/ream /app/ream

# Use Ubuntu as the release image
FROM ubuntu AS runtime
WORKDIR /app

# Copy reth over from the build stage
COPY --from=builder /app/ream /usr/local/bin

# Copy licenses
COPY LICENSE ./

EXPOSE 8545 8546
ENTRYPOINT ["/usr/local/bin/ream"]
