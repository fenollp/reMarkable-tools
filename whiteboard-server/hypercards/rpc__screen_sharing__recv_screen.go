package hypercards

import (
	"context"
	"time"

	"go.uber.org/zap"
)

func (srv *Server) validateRecvScreen(ctx context.Context, req *RecvScreenReq) (ok bool) {
	if req.GetRoomId() == "" {
		return
	}
	if ntui(req.GetRoomId()) != nil {
		return
	}
	return true
}

// RecvScreen ...
func (srv *Server) RecvScreen(ctx context.Context, req *RecvScreenReq) (rep *RecvScreenRep, err error) {
	ctx, cancel, err := srv.prepare(ctx, optAllowAnons())
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling RecvScreen")
	start := time.Now()

	if !srv.validateRecvScreen(ctx, req) {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}

	var png []byte
	if png, err = srv.red(ctx).getScreenSharing(ctx, req.GetRoomId() /*, req.GetPassword()*/); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	rep = &RecvScreenRep{
		CanvasPng: png,
	}
	log.Info("handled RecvScreen", zap.Duration("in", time.Since(start)))
	return
}
