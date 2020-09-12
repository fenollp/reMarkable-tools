package hypercard_whiteboard

import (
	"context"
	"fmt"
	"time"

	"github.com/devimteam/amqp/conn"
	"github.com/devimteam/amqp/logger"
	"github.com/streadway/amqp"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

const (
	rabbitApp        = "hypercard_whiteboard"
	rabbitAckTimeout = 15 * time.Minute
)

var (
	rabbitExchange, rabbitExchangeType string
)

type rabbitService struct {
	coSub, coPub                                    *conn.Connection
	rabbitHost, rabbitUser, rabbitPass, rabbitVhost string
	rabbitURI                                       string
}

func (rmq *rabbitService) close(ctx context.Context) {
	log := NewLogFromCtx(ctx)
	log.Info("closing rabbitmq subs conn")
	if err := rmq.coSub.Close(); err != nil {
		log.Error("", zap.Error(err))
	}
	log.Info("closing rabbitmq pubs conn")
	if err := rmq.coPub.Close(); err != nil {
		log.Error("", zap.Error(err))
	}
}

func (srv *Server) setupRabbit(ctx context.Context, xName, user, pass, vhost, host string) (err error) {
	rabbitExchange = xName
	rabbitExchangeType = amqp.ExchangeTopic
	rmq := &rabbitService{
		rabbitUser:  user,
		rabbitPass:  pass,
		rabbitVhost: vhost,
		rabbitHost:  host,
	}
	rmq.rabbitURI = "amqp://" + user + ":" + pass + "@" + host + ":5672"
	if vhost != "/" {
		rmq.rabbitURI += "/" + vhost
	}

	log := NewLogFromCtx(ctx)
	log.Info("dialing", zap.String("uri", rmq.rabbitURI))
	start := time.Now()
	rmq.coPub = conn.Connect(rmq.rabbitURI,
		// For connection status and errors
		conn.WithLogger(rmq.newChanLogger(ctx, "pub")),
	)
	log.Info("connected pub to rabbitmq", zap.Duration("in", time.Since(start)))
	go func() {
		log.Debug("registering pub's NotifyClose")
		sig := <-rmq.coPub.NotifyClose()
		log.Error("", zap.Any("sig", sig))
	}()

	start = time.Now()
	rmq.coSub = conn.Connect(rmq.rabbitURI,
		// For connection status and errors
		conn.WithLogger(rmq.newChanLogger(ctx, "sub")),
	)
	log.Info("connected sub to rabbitmq", zap.Duration("in", time.Since(start)))
	go func() {
		log.Debug("registering sub's NotifyClose")
		sig := <-rmq.coSub.NotifyClose()
		log.Error("", zap.Any("sig", sig))
	}()

	var c *rabbitClient
	if c, err = rmq.newSubClient(ctx); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	defer c.close(ctx)

	log.Debug("declaring exchange", zap.String("name", rabbitExchange), zap.String("type", rabbitExchangeType))
	start = time.Now()
	if err = c.ch.ExchangeDeclare(
		rabbitExchange,
		rabbitExchangeType,
		false, // durable
		false, // delete when complete
		false, // internal
		false, // noWait
		nil,   // arguments
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Info("declared exchange. Ready!", zap.Duration("in", time.Since(start)))
	srv.rmq = rmq
	return

}

func (rmq *rabbitService) newChanLogger(ctx context.Context, kind string) logger.Logger {
	log := NewLogFromCtx(ctx)
	ch := make(chan []interface{})
	go func() {
		for things := range ch {
			ls := make([]zapcore.Field, 0, len(things))
			for i, thing := range things {
				k := fmt.Sprintf("%d", i)
				ls = append(ls, zap.Any(k, thing))
			}
			log.Info(kind, ls...)
		}
	}()
	return logger.NewChanLogger(ch)
}
