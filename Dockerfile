# syntax=docker/dockerfile:1
#
# The hardless/hardless image (NFR-09, NFR-12): the static `hardless` binary on distroless.
#
# It packs prebuilt static binaries, one per platform, so building for linux/amd64 and
# linux/arm64 needs no emulation:
#
#   docker/bin/amd64/hardless   cargo build --release -p hardless-cli --target x86_64-unknown-linux-musl
#   docker/bin/arm64/hardless   cargo build --release -p hardless-cli --target aarch64-unknown-linux-musl
#
# Build the dashboard first (`pnpm --filter @hardless/dashboard build`): it is embedded in the
# binary. CI does all of this in .github/workflows/docker.yml. Then:
#
#   docker buildx build --platform linux/amd64,linux/arm64 -t hardless/hardless .
#   docker run -p 8080:8080 -p 8443:8443 -v hardless-data:/data hardless/hardless

# An empty /data owned by the image's user. This stage runs on the build machine's own
# platform, so it needs no emulation either.
FROM --platform=$BUILDPLATFORM busybox:1.37 AS data
RUN mkdir -p /data/certs

FROM gcr.io/distroless/static-debian12:nonroot
ARG TARGETARCH
COPY --chmod=0755 docker/bin/${TARGETARCH}/hardless /hardless
COPY --from=data --chown=65532:65532 /data /data
# Listen on every interface so `-p` works. The local certificate authority is made on first
# run in /data/certs (NFR-05: never in the image); keep /data in a volume so devices that
# trusted it once keep trusting it. For phones on the LAN, add the host's address to the
# certificate: -e HARDLESS_CERT_HOSTS=192.168.1.20
ENV HARDLESS_HOST=0.0.0.0 \
    HARDLESS_CERT_DIR=/data/certs
EXPOSE 8080 8443
VOLUME ["/data"]
USER nonroot
ENTRYPOINT ["/hardless"]
CMD ["start"]
