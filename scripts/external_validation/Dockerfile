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
    PRE_COMMIT_HOME=/opt/pre-commit-cache \
    CARGO_NET_OFFLINE=true \
    PIP_DISABLE_PIP_VERSION_CHECK=1

RUN apt-get update && apt-get install -y --no-install-recommends \
      bash ca-certificates coreutils diffutils findutils git grep jq \
      python3 python3-pip python3-venv sed util-linux \
      build-essential curl pkg-config libasound2-dev libudev-dev libwayland-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=reprocut-builder /src/target/release/reprocut /opt/reprocut/reprocut
COPY base/ /inputs/base/
COPY head/ /inputs/head/
COPY case.json /case.json
COPY reprocut/scripts/external_validation/container_entrypoint.sh /usr/local/bin/reprocut-validation

RUN set -eux; \
    case "$CASE_ID" in \
      openruyi) \
        python3 -m venv /opt/precommit; \
        /opt/precommit/bin/pip install --no-cache-dir pre-commit; \
        mkdir -p /opt/wheelhouse /tmp/openruyi-warm; \
        /opt/precommit/bin/pip wheel --wheel-dir /opt/wheelhouse /inputs/head; \
        /opt/precommit/bin/pip download --dest /opt/wheelhouse setuptools wheel pip; \
        cp -a /inputs/head/. /tmp/openruyi-warm/; \
        cd /tmp/openruyi-warm; \
        git init -q; git add -A; \
        PRE_COMMIT_HOME=/opt/pre-commit-cache /opt/precommit/bin/pre-commit run end-of-file-fixer --all-files || true; \
        printf '%s\n' '{"schema":1,"commands":[["git","init","-q"],["git","add","-A"]]}' > /opt/openruyi-prepare.json; \
        rm -rf /tmp/openruyi-warm ;; \
      ipe) \
        true ;; \
      bevy) \
        export CARGO_NET_OFFLINE=false; \
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable --component clippy; \
        cd /inputs/head; cargo fetch --locked; \
        cd /inputs/base; cargo fetch --locked ;; \
      *) \
        echo "unsupported CASE_ID: $CASE_ID" >&2; exit 2 ;; \
    esac; \
    chmod 0555 /opt/reprocut/reprocut /usr/local/bin/reprocut-validation; \
    useradd --uid 10001 --user-group --create-home --shell /usr/sbin/nologin validator; \
    chown -R root:root /inputs /opt/reprocut /opt/precommit /opt/pre-commit-cache /opt/wheelhouse /opt/cargo /opt/rustup 2>/dev/null || true; \
    chmod -R a+rX,a-w /inputs /opt/reprocut /opt/precommit /opt/pre-commit-cache /opt/wheelhouse /opt/cargo /opt/rustup 2>/dev/null || true; \
    rm -rf /root/.cache /tmp/*

USER 10001:10001
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/reprocut-validation"]
