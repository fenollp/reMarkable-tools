package hypercards

import (
	"context"
	"time"

	"go.uber.org/zap"
)

// ListRooms ...
func (srv *Server) ListRooms(ctx context.Context, req *ListRoomsReq) (rep *ListRoomsRep, err error) {
	ctx, cancel, err := srv.prepare(ctx)
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling ListRooms")
	start := time.Now()

	// TODO

	rep = &ListRoomsRep{}
	log.Info("handled ListRooms", zap.Duration("in", time.Since(start)))
	return
}
