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
RUN if [ -n "$FEATURES" ]; then \
      cargo chef cook --profile $BUILD_PROFILE --no-default-features --features "$FEATURES" --recipe-path recipe.json; \
    else \
      cargo chef cook --profile $BUILD_PROFILE --recipe-path recipe.json; \
    fi

# Build application
COPY --exclude=.git --exclude=dist . .
RUN if [ -n "$FEATURES" ]; then \
      cargo build --profile $BUILD_PROFILE --no-default-features --features "$FEATURES" --locked --bin ream; \
    else \
      cargo build --profile $BUILD_PROFILE --locked --bin ream; \
    fi

# ARG is not resolved in COPY so we have to hack around it by copying the
# binary to a temporary location
RUN cp /app/target/$BUILD_PROFILE/ream /app/ream

# Save the exact leanMultisig checkout path so we can replicate it in the runtime image
RUN ls -d /usr/local/cargo/git/checkouts/leanmultisig-*/* > /app/leanmultisig-path.txt && \
    cp -r $(cat /app/leanmultisig-path.txt) /app/leanmultisig-checkout

# Use Ubuntu as the release image
FROM ubuntu AS runtime
WORKDIR /app

# Copy ream over from the build stage
COPY --from=builder /app/ream /usr/local/bin

# Restore leanMultisig checkout at original path for aggregation bytecode runtime fingerprint check
COPY --from=builder /app/leanmultisig-checkout /app/leanmultisig-checkout
COPY --from=builder /app/leanmultisig-path.txt /app/leanmultisig-path.txt
RUN mkdir -p $(cat /app/leanmultisig-path.txt) && \
    cp -r /app/leanmultisig-checkout/* $(cat /app/leanmultisig-path.txt)/ && \
    rm -rf /app/leanmultisig-checkout /app/leanmultisig-path.txt

# Copy licenses
COPY LICENSE ./

EXPOSE 9000/udp 5052 8080
ENTRYPOINT ["/usr/local/bin/ream"]
