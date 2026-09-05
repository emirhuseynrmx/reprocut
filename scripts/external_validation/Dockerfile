# syntax=docker/dockerfile:1.7
#
# Both stages are pinned by digest, not tag. A tag is mutable, so a rebuilt
# `rust:1.85-bookworm` would silently change the toolchain that produced the
# reducer whose evidence this image exists to make reproducible.
FROM rust:1.85-bookworm@sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4 AS reprocut-builder

WORKDIR /src
COPY reprocut/ ./
RUN cargo build --locked --release -p reprocut

FROM ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517 AS validation-runtime

ARG CASE_ID
ENV DEBIAN_FRONTEND=noninteractive \
    PATH=/opt/precommit/bin:/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    CARGO_NET_OFFLINE=true \
    PIP_DISABLE_PIP_VERSION_CHECK=1

RUN apt-get update && apt-get install -y --no-install-recommends \
      bash ca-certificates coreutils diffutils findutils git grep jq ripgrep \
      python3 python3-pip python3-venv sed util-linux \
      build-essential curl pkg-config libasound2-dev libudev-dev libwayland-dev \
      libxkbcommon-dev libx264-dev libfontconfig1-dev clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=reprocut-builder /src/target/release/reprocut /opt/reprocut/reprocut
COPY base/ /inputs/base/
COPY head/ /inputs/head/
COPY case.json /case.json
COPY reprocut/scripts/external_validation/container_entrypoint.sh /usr/local/bin/reprocut-validation
COPY reprocut/scripts/external_validation/openruyi_eof_oracle.sh /usr/local/bin/openruyi-eof-oracle
COPY reprocut/scripts/external_validation/ipe_regen_oracle.sh /usr/local/bin/ipe-regen-oracle
COPY reprocut/scripts/external_validation/bevy_clippy_oracle.sh /usr/local/bin/bevy-clippy-oracle

# rustup is fetched as a pinned, checksum-verified binary rather than by piping
# https://sh.rustup.rs into a shell. That endpoint serves whatever installer is
# current, so the toolchain that observed a failure could change between replays
# of the same pinned case.
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256_X86_64=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ARG RUSTUP_SHA256_AARCH64=15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433

RUN set -eux; \
    install_rustup() { \
      case "$(uname -m)" in \
        x86_64) target=x86_64-unknown-linux-gnu; expected="$RUSTUP_SHA256_X86_64" ;; \
        aarch64) target=aarch64-unknown-linux-gnu; expected="$RUSTUP_SHA256_AARCH64" ;; \
        *) echo "unsupported architecture: $(uname -m)" >&2; exit 2 ;; \
      esac; \
      curl --retry 5 --retry-all-errors --proto '=https' --tlsv1.2 -sSf \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${target}/rustup-init" \
        -o /tmp/rustup-init; \
      echo "${expected}  /tmp/rustup-init" | sha256sum -c -; \
      chmod +x /tmp/rustup-init; \
      /tmp/rustup-init -y --no-modify-path --default-toolchain none; \
      rm -f /tmp/rustup-init; \
    }; \
    case "$CASE_ID" in \
      openruyi) \
        python3 -m venv /opt/precommit; \
        /opt/precommit/bin/pip install --no-cache-dir pre-commit-hooks==6.0.0 ;; \
      ipe) \
        export CARGO_NET_OFFLINE=false; \
        install_rustup; \
        rustup toolchain install stable --profile minimal; \
        rustup default stable; \
        cp -R /inputs/base /tmp/ipe-base-build; \
        cd /tmp/ipe-base-build; cargo build --release -p ipe; \
        install -m 0555 target/release/ipe /usr/local/bin/ipe-base; \
        cd /inputs/head; cargo build --locked --release -p ipe; \
        install -m 0555 target/release/ipe /usr/local/bin/ipe-head ;; \
      bevy) \
        export CARGO_NET_OFFLINE=false; \
        install_rustup; \
        rustup toolchain install stable --profile minimal --component clippy --component rustfmt; \
        rustup default stable; \
        cd /inputs/head; if [ -f Cargo.lock ]; then cargo fetch --locked; else cargo fetch; fi; \
        cd /inputs/base; if [ -f Cargo.lock ]; then cargo fetch --locked; else cargo fetch; fi ;; \
      *) \
        echo "unsupported CASE_ID: $CASE_ID" >&2; exit 2 ;; \
    esac; \
    chmod 0555 /opt/reprocut/reprocut /usr/local/bin/reprocut-validation /usr/local/bin/openruyi-eof-oracle /usr/local/bin/ipe-regen-oracle /usr/local/bin/bevy-clippy-oracle; \
    useradd --uid 10001 --user-group --create-home --shell /usr/sbin/nologin validator; \
    install -d -o 10001 -g 10001 -m 0700 /evidence; \
    chown -R root:root /inputs /opt/reprocut /opt/precommit /opt/cargo /opt/rustup 2>/dev/null || true; \
    chmod -R a+rX,a-w /inputs /opt/reprocut /opt/precommit /opt/cargo /opt/rustup 2>/dev/null || true; \
    rm -rf /root/.cache /tmp/*

USER 10001:10001
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/reprocut-validation"]
