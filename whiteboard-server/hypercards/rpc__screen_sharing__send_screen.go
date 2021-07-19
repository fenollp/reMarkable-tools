package hypercards

import (
	"context"
	"time"

	"go.uber.org/zap"
)

func (srv *Server) validateSendScreen(ctx context.Context, req *SendScreenReq) (ok bool) {
	canvas := req.GetScreenPng()
	if canvas == nil {
		return
	}
	if req.GetRoomId() == "" {
		return
	}
	if ntui(req.GetRoomId()) != nil {
		return
	}
	return true
}

// SendScreen ...
func (srv *Server) SendScreen(ctx context.Context, req *SendScreenReq) (rep *SendScreenRep, err error) {
	ctx, cancel, err := srv.prepare(ctx)
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling RecvScreen")
	start := time.Now()

	if !srv.validateSendScreen(ctx, req) {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}

	if err = srv.red(ctx).setScreenSharing(ctx, req.GetRoomId(), req.GetScreenPng()); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	rep = &SendScreenRep{}
	log.Info("handled SendScreen", zap.Duration("in", time.Since(start)))
	return
}
