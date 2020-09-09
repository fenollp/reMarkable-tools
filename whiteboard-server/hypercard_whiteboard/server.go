package hypercard_whiteboard

import (
	"context"
	"os"
	"time"

	"go.uber.org/zap"
)

var _ WhiteboardServer = &Server{} // Ensures all RPCs are implemented

// Server holds connections to our services accessible by gRPC rpcs.
type Server struct {
	rmq *rabbitService
}

// Close ...
func (srv *Server) Close(ctx context.Context) {
	log := NewLogFromCtx(ctx)
	// Shutdown server's services here
	log.Info("closing rabbit conns")
	srv.rmq.close(ctx)
}

// NewServer opens connections to our services
func NewServer(ctx context.Context) (srv *Server, err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()

	srv = &Server{}

	// Start server's services here (Redis, RMQ, ...)

	if err = srv.setupRabbit(ctx,
		os.Getenv("RABBITMQ_EXCHANGE"),
		os.Getenv("RABBITMQ_USER"),
		os.Getenv("RABBITMQ_PASS"),
		os.Getenv("RABBITMQ_VHOST"),
		os.Getenv("RABBITMQ_HOST"),
	); err != nil {
		return
	}

	log.Info("server ready", zap.Duration("in", time.Since(start)))
	return
}
