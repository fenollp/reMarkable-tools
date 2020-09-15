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
	nc *natsClient
}

// Close ...
func (srv *Server) Close(ctx context.Context) {
	log := NewLogFromCtx(ctx)
	// Shutdown server's services here
	log.Info("closing nats conn")
	srv.nc.Close()
}

// NewServer opens connections to our services
func NewServer(ctx context.Context) (srv *Server, err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()

	srv = &Server{}

	// Start server's services here (Redis, RMQ, ...)

	if err = srv.setupNats(ctx,
		"nats",
		os.Getenv("NATS_USER"),
		os.Getenv("NATS_PASS"),
	); err != nil {
		return
	}

	log.Info("server ready", zap.Duration("in", time.Since(start)))
	return
}
