package hypercard_whiteboard

import (
	"context"
	"fmt"
	"time"

	"github.com/golang/protobuf/proto"
	"go.uber.org/zap"
)

func (srv *Server) validateRecvEvent(ctx context.Context, req *RecvEventsReq) error {
	// TODO: disallow RabbitMQ special chars (. # *) from roomID
	if roomID := req.GetRoomId(); roomID == "" {
		return errBadRequest
	}
	return nil
}

// RecvEvents ...
func (srv *Server) RecvEvents(req *RecvEventsReq, stream Whiteboard_RecvEventsServer) (err error) {
	ctx, cancel, err := srv.prepare(stream.Context(), optNoDeadline())
	defer cancel()
	if err != nil {
		return
	}
	log := NewLogFromCtx(ctx)
	log.Info("handling RecvEvent")
	start := time.Now()

	if err = srv.validateRecvEvent(ctx, req); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	bk := rkOfEvent{
		roomID: req.GetRoomId(),
		userID: "*",
		kind:   "*",
	}.String()
	queue := fmt.Sprintf("sub.%s.%s", bk, ctxUID(ctx))

	log.Debug("listening for events", zap.String("bk", bk), zap.String("q", queue))
	var c *rabbitClient
	if c, err = srv.rmq.newSubClient(ctx); err != nil {
		return
	}
	defer c.close(ctx)

	if _, err = c.qDeclare(ctx, queue); err != nil {
		return
	}
	if err = c.qBind(ctx, queue, bk); err != nil {
		return
	}
	deliveries, cancel, err := c.qConsume(ctx, queue)
	if err != nil {
		log.Error("", zap.Error(err))
		return
	}
	defer cancel()

	// Join event
	{
		event := &Event{
			CreatedAt:              time.Now().UnixNano(),
			RoomId:                 req.GetRoomId(),
			UserId:                 ctxUID(ctx),
			EventUserJoinedTheRoom: true,
		}
		rk := event.rk()
		log.Debug("publishing", zap.String("rk", rk))
		if err = c.publish(ctx, rk, event); err != nil {
			return
		}
	}

	// Leave event
	defer func() {
		event := &Event{
			CreatedAt:            time.Now().UnixNano(),
			RoomId:               req.GetRoomId(),
			UserId:               ctxUID(ctx),
			EventUserLeftTheRoom: true,
		}
		rk := event.rk()
		log.Debug("publishing", zap.String("rk", rk))
		if err = c.publish(ctx, rk, event); err != nil {
			return
		}
	}()

	for {
		select {
		case <-ctx.Done():
			if err = ctx.Err(); err == context.Canceled {
				log.Info("ctx canceled")
			} else {
				log.Error("", zap.Error(err))
			}
			return

		case d := <-deliveries:
			if err = d.Ack(false); err != nil {
				log.Error("while ack-ing", zap.Error(err))
				return
			}
			rk := d.RoutingKey

			o, ok := fromRK(rk).(rkOfEvent)
			if !ok {
				err = fmt.Errorf("bad rkOfEvent: %q", rk)
				log.Error("", zap.Error(err), zap.Any("o", o))
				return
			}
			if o.userID == ctxUID(ctx) {
				log.Debug("not FWDing to self")
				continue
			}

			var event Event
			{
				start := time.Now()
				if err = proto.Unmarshal(d.Body, &event); err != nil {
					log.Error("", zap.Error(err))
					continue
				}
				log.Debug("decoded", zap.Duration("in", time.Since(start)))
			}

			start = time.Now()
			if err = stream.Send(&event); err != nil {
				log.Error("", zap.Error(err))
				return
			}
			log.Debug("forwarded event",
				zap.String("rk", rk),
				zap.Duration("in", time.Since(start)),
			)
		}
	}
	// rep = LINTING (disregard)
}
