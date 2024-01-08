# syntax=docker.io/docker/dockerfile:1@sha256:ac85f380a63b13dfcefa89046420e1781752bab202122f8f50032edf31be0021

FROM --platform=$BUILDPLATFORM docker.io/library/golang:1-alpine@sha256:4db4aac30880b978cae5445dd4a706215249ad4f43d28bd7cdf7906e9be8dd6b AS builder
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
