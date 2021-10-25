# syntax=docker.io/docker/dockerfile:1@sha256:42399d4635eddd7a9b8a24be879d2f9a930d0ed040a61324cfdf59ef1357b3b2

FROM --platform=$BUILDPLATFORM docker.io/library/golang:1-alpine@sha256:5519c8752f6b53fc8818dc46e9fda628c99c4e8fd2d2f1df71e1f184e71f47dc AS builder
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
