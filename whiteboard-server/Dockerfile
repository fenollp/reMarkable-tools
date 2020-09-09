# Not using golang:1-alpine because of https://github.com/golang/go/issues/35777
FROM golang:1.13-alpine AS builder
# FROM golang:1-alpine AS builder
RUN set -ux \
 && apk update \
 && apk add ca-certificates git tzdata \
 && update-ca-certificates

WORKDIR /app
COPY go.mod go.sum ./
RUN set -ux \
 && go mod download \
 && go mod verify
COPY . .
RUN set -ux \
 && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -mod=readonly -o whiteboard-server -ldflags '-s -w'

FROM scratch
WORKDIR /
COPY --from=builder /usr/share/zoneinfo /usr/share/zoneinfo
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /app/whiteboard-server /whiteboard-server
EXPOSE 10000/tcp
ENTRYPOINT ["/whiteboard-server"]
