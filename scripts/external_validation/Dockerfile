# syntax=docker/dockerfile:1.7
FROM rust:1.85-bookworm AS reprocut-builder

WORKDIR /src
COPY reprocut/ ./
RUN cargo build --locked --release -p reprocut

FROM ubuntu:24.04 AS validation-runtime

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
      libxkbcommon-dev libx264-dev libfontconfig1-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=reprocut-builder /src/target/release/reprocut /opt/reprocut/reprocut
COPY base/ /inputs/base/
COPY head/ /inputs/head/
COPY case.json /case.json
COPY reprocut/scripts/external_validation/container_entrypoint.sh /usr/local/bin/reprocut-validation
COPY reprocut/scripts/external_validation/openruyi_eof_oracle.sh /usr/local/bin/openruyi-eof-oracle
COPY reprocut/scripts/external_validation/ipe_regen_oracle.sh /usr/local/bin/ipe-regen-oracle

RUN set -eux; \
    case "$CASE_ID" in \
      openruyi) \
        python3 -m venv /opt/precommit; \
        /opt/precommit/bin/pip install --no-cache-dir pre-commit-hooks==6.0.0 ;; \
      ipe) \
        export CARGO_NET_OFFLINE=false; \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable; \
        cp -R /inputs/base /tmp/ipe-base-build; \
        cd /tmp/ipe-base-build; cargo build --release -p ipe; \
        install -m 0555 target/release/ipe /usr/local/bin/ipe-base; \
        cd /inputs/head; cargo build --locked --release -p ipe; \
        install -m 0555 target/release/ipe /usr/local/bin/ipe-head ;; \
      bevy) \
        export CARGO_NET_OFFLINE=false; \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --component clippy --component rustfmt; \
        cd /inputs/head; if [ -f Cargo.lock ]; then cargo fetch --locked; else cargo fetch; fi; \
        cd /inputs/base; if [ -f Cargo.lock ]; then cargo fetch --locked; else cargo fetch; fi ;; \
      *) \
        echo "unsupported CASE_ID: $CASE_ID" >&2; exit 2 ;; \
    esac; \
    chmod 0555 /opt/reprocut/reprocut /usr/local/bin/reprocut-validation /usr/local/bin/openruyi-eof-oracle /usr/local/bin/ipe-regen-oracle; \
    useradd --uid 10001 --user-group --create-home --shell /usr/sbin/nologin validator; \
    install -d -o 10001 -g 10001 -m 0700 /evidence; \
    chown -R root:root /inputs /opt/reprocut /opt/precommit /opt/cargo /opt/rustup 2>/dev/null || true; \
    chmod -R a+rX,a-w /inputs /opt/reprocut /opt/precommit /opt/cargo /opt/rustup 2>/dev/null || true; \
    rm -rf /root/.cache /tmp/*

USER 10001:10001
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/reprocut-validation"]
