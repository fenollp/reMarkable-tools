package hypercards

import (
	"context"
	"os"
	"strconv"
	"time"

	"go.uber.org/zap"
)

var _ WhiteboardServer = &Server{}    // Ensures all RPCs are implemented
var _ ScreenSharingServer = &Server{} // Ensures all RPCs are implemented

// Server holds connections to our services accessible by gRPC rpcs.
type Server struct {
	nc *natsClient
	rc *redisClient
}

// Close ...
func (srv *Server) Close(ctx context.Context) {
	log := NewLogFromCtx(ctx)
	// Shutdown server's services here
	log.Info("closing nats conn")
	srv.nc.Close()
}

// NewServer opens connections to our services
func NewServer(ctx context.Context, onlyRedis bool) (srv *Server, err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()

	srv = &Server{}

	// Start server's services here (Redis, RMQ, ...)

	if !onlyRedis {

		if err = srv.setupNats(ctx,
			"nats",
			os.Getenv("NATS_USER"),
			os.Getenv("NATS_PASS"),
		); err != nil {
			return
		}

	}

	var redisDB int
	if redisDB, err = strconv.Atoi(os.Getenv("REDIS_DB")); err != nil {
		return
	}
	if err = srv.setupRedis(ctx,
		os.Getenv("REDIS_HOST"),
		os.Getenv("REDIS_PORT"),
		os.Getenv("REDIS_PASSWORD"),
		redisDB,
	); err != nil {
		return
	}

	log.Info("server ready", zap.Duration("in", time.Since(start)))
	return
}
