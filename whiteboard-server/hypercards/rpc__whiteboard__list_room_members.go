package hypercards

import (
	"context"
	"time"

	"go.uber.org/zap"
)

// ListRoomMembers ...
func (srv *Server) ListRoomMembers(ctx context.Context, req *ListRoomMembersReq) (rep *ListRoomMembersRep, err error) {
	ctx, cancel, err := srv.prepare(ctx)
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling ListRoomMembers")
	start := time.Now()

	// TODO

	rep = &ListRoomMembersRep{}
	log.Info("handled ListRoomMembers", zap.Duration("in", time.Since(start)))
	return
}
