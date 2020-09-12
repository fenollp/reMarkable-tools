package hypercard_whiteboard

import (
	"context"
	"errors"
	"time"

	"go.uber.org/zap"
)

var errBadRequest = errors.New("bad request")

func (srv *Server) validateSendEvent(ctx context.Context, req *SendEventReq) error {
	event := req.GetEvent()
	if createdAt := event.GetCreatedAt(); createdAt != 0 {
		return errBadRequest
	}
	if userID := event.GetUserId(); userID != "" {
		return errBadRequest
	}
	if roomID := event.GetRoomId(); roomID != "" {
		return errBadRequest
	}
	if !xorN(
		event.GetEventDrawing() != nil,
		event.GetEventUserLeftTheRoom() != false,
		event.GetEventUserJoinedTheRoom() != false,
	) {
		return errBadRequest
	}

	roomIDs := req.GetRoomIds()
	if hasDuplicates(roomIDs) {
		return errBadRequest
	}
	for _, roomID := range roomIDs {
		if err := ntui(roomID); err != nil {
			return err
		}
	}

	if drawing := event.GetEventDrawing(); drawing != nil {
		if drawing.GetColor() == Drawing_invisible {
			return errBadRequest
		}
		if len(drawing.GetXs()) == 0 ||
			len(drawing.GetXs()) != len(drawing.GetYs()) ||
			len(drawing.GetXs()) != len(drawing.GetPressures()) ||
			len(drawing.GetXs()) != len(drawing.GetWidths()) {
			return errBadRequest
		}
	}
	return nil
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

	var c *rabbitClient
	if c, err = srv.rmq.newPubClient(ctx); err != nil {
		return
	}
	defer c.close(ctx)

	for _, roomID := range req.GetRoomIds() {
		event := &Event{
			CreatedAt:              time.Now().UnixNano(),
			UserId:                 ctxUID(ctx),
			RoomId:                 roomID,
			EventDrawing:           req.GetEvent().GetEventDrawing(),
			EventUserLeftTheRoom:   req.GetEvent().GetEventUserLeftTheRoom(),
			EventUserJoinedTheRoom: req.GetEvent().GetEventUserJoinedTheRoom(),
		}

		rk := event.rk()
		log.Debug("publishing", zap.String("rk", rk))
		if err = c.publish(ctx, rk, event); err != nil {
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
