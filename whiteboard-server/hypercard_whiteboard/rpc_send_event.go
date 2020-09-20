package hypercard_whiteboard

import (
	"context"
	"errors"
	"time"

	"go.uber.org/zap"
)

var errBadRequest = errors.New("bad request")

func (srv *Server) validateSendEvent(ctx context.Context, req *SendEventReq) (err error) {
	log := NewLogFromCtx(ctx)

	event := req.GetEvent()
	if createdAt := event.GetCreatedAt(); createdAt != 0 {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}
	if userID := event.GetByUserId(); userID != "" {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}
	if roomID := event.GetInRoomId(); roomID != "" {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}
	switch event.GetEvent().(type) {
	case *Event_Drawing:
		drawing := event.GetDrawing()
		if drawing.GetColor() == Drawing_invisible {
			err = errBadRequest
			log.Error("", zap.Error(err))
			return
		}
		if len(drawing.GetXs()) == 0 ||
			len(drawing.GetXs()) != len(drawing.GetYs()) ||
			len(drawing.GetXs()) != len(drawing.GetPressures()) ||
			len(drawing.GetXs()) != len(drawing.GetWidths()) {
			err = errBadRequest
			log.Error("", zap.Error(err))
			return
		}

	// Disallow status events
	case *Event_UserJoinedTheRoom, *Event_UserLeftTheRoom:
		err = errBadRequest
		log.Error("", zap.Error(err))
		return

	default:
		log.Debug("unhandled event", zap.Any("event", event.GetEvent()))
	}

	roomIDs := req.GetRoomIds()
	if hasDuplicates(roomIDs) {
		err = errBadRequest
		log.Error("", zap.Error(err))
		return
	}
	for _, roomID := range roomIDs {
		if err = ntui(roomID); err != nil {
			log.Error("", zap.Error(err))
			return
		}
	}
	return
}

// SendEvent ...
func (srv *Server) SendEvent(ctx context.Context, req *SendEventReq) (rep *SendEventRep, err error) {
	ctx, cancel, err := srv.prepare(ctx)
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling SendEvent")
	start := time.Now()

	if err = srv.validateSendEvent(ctx, req); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	event := &Event{
		CreatedAt: time.Now().UnixNano(),
		ByUserId:  ctxUID(ctx),
		Event:     req.GetEvent().GetEvent(),
	}
	for _, roomID := range req.GetRoomIds() {
		event.InRoomId = roomID
		if err = srv.nc.publish(ctx, event); err != nil {
			return
		}
	}

	rep = &SendEventRep{}
	log.Info("handled SendEvent", zap.Duration("in", time.Since(start)))
	return
}

func hasDuplicates(strs []string) bool {
	set := make(map[string]struct{}, len(strs))
	for _, str := range strs {
		set[str] = struct{}{}
	}
	return len(strs) != len(set)
}
