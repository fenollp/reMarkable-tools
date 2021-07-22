# syntax=docker.io/docker/dockerfile:1.2@sha256:e2a8561e419ab1ba6b2fe6cbdf49fd92b95912df1cf7d313c3e2230a333fdbcc

FROM --platform=$BUILDPLATFORM docker.io/library/golang:1-alpine@sha256:3b11fcdf0d8436e1a5ba3a8bf5206c7f28f5a652ac899f2107ebe127b668bcfa AS builder
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
