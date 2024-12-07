# syntax=docker.io/docker/dockerfile:1@sha256:865e5dd094beca432e8c0a1d5e1c465db5f998dca4e439981029b3b81fb39ed5

FROM --platform=$BUILDPLATFORM docker.io/library/golang:1-alpine@sha256:6c5c9590f169f77c8046e45c611d3b28fe477789acd8d3762d23d4744de69812 AS builder
RUN \
  --mount=type=cache,target=/var/cache/apk ln -vs /var/cache/apk /etc/apk/cache && \
    set -ux \
 && apk update
WORKDIR /app
COPY go.mod go.sum ./
RUN \
  --mount=type=cache,target=/go/pkg/mod \
  --mount=type=cache,target=/root/.cache/go-build \
    set -ux \
 && go mod download \
 && go mod verify
COPY . .

FROM builder AS builder-grpc
RUN \
  --mount=type=cache,target=/go/pkg/mod \
  --mount=type=cache,target=/root/.cache/go-build \
    set -ux \
 && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -mod=readonly -o grpc-server -ldflags '-s -w' cmd/grpc-server/grpc.go

FROM builder AS builder-http
RUN \
  --mount=type=cache,target=/go/pkg/mod \
  --mount=type=cache,target=/root/.cache/go-build \
    set -ux \
 && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -mod=readonly -o http-server -ldflags '-s -w' cmd/http-server/http.go

FROM scratch AS grpc-server
COPY --from=builder-grpc /app/grpc-server /grpc-server
ENTRYPOINT ["/grpc-server"]

FROM scratch AS http-server
COPY --from=builder-http /app/http-server /http-server
ENTRYPOINT ["/http-server"]
