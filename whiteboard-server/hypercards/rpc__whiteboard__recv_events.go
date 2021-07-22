package hypercards

import (
	"context"
	"fmt"
	"time"

	"github.com/golang/protobuf/proto"
	nats "github.com/nats-io/nats.go"
	"go.uber.org/zap"
)

func (srv *Server) validateRecvEvents(ctx context.Context, req *RecvEventsReq) error {
	roomID := req.GetRoomId()
	if roomID == "" {
		return errBadRequest
	}
	if err := ntui(roomID); err != nil {
		return err
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
	log.Info("handling RecvEvents")
	start := time.Now()

	if err = srv.validateRecvEvents(ctx, req); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	bk := rkOfEvent{
		roomID: req.GetRoomId(),
		userID: "*",
		kind:   "*",
	}.String()

	log.Debug("listening for events", zap.String("bk", bk))
	var (
		deliveries = make(chan *nats.Msg)
		sub        *nats.Subscription
	)
	if sub, err = srv.nc.ChanSubscribe(bk, deliveries); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	defer sub.Unsubscribe()

	// Join event
	{
		event := &Event{
			CreatedAt: start.UnixNano(),
			InRoomId:  req.GetRoomId(),
			ByUserId:  ctxUID(ctx),
			Event:     &Event_UserJoinedTheRoom{UserJoinedTheRoom: true},
		}
		if err = srv.nc.publish(ctx, event); err != nil {
			return
		}
	}

	// Leave event
	defer func() {
		event := &Event{
			CreatedAt: time.Now().UnixNano(),
			InRoomId:  req.GetRoomId(),
			ByUserId:  ctxUID(ctx),
			Event:     &Event_UserLeftTheRoom{UserLeftTheRoom: true},
		}
		if err = srv.nc.publish(ctx, event); err != nil {
			return
		}
	}()

	{
		var count uint32
		if count, err = srv.nc.countUsersInRoom(ctx, bk); err != nil {
			log.Error("", zap.Error(err))
			return
		}
		event := &Event{
			CreatedAt: time.Now().UnixNano(),
			InRoomId:  req.GetRoomId(),
			ByUserId:  ctxUID(ctx),
			Event:     &Event_UsersInTheRoom{UsersInTheRoom: count},
		}
		start = time.Now()
		if err = stream.Send(event); err != nil {
			log.Error("", zap.Error(err))
			return
		}
		log.Debug("sent count event", zap.Duration("in", time.Since(start)))
	}

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
			rk := d.Subject

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
				if err = proto.Unmarshal(d.Data, &event); err != nil {
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
