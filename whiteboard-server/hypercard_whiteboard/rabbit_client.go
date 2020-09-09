package hypercard_whiteboard

import (
	"context"
	"fmt"
	"time"

	"github.com/golang/protobuf/proto"
	"github.com/streadway/amqp"
	"go.uber.org/zap"
)

const rabbitMessageExpiration = int32((5 * time.Minute) / time.Millisecond)

var rabbitMessageExpirationS = fmt.Sprintf("%d", rabbitMessageExpiration)

type rabbitClient struct {
	ch          *amqp.Channel
	confirms    chan amqp.Confirmation
	deliveryTag uint64
}

func (c *rabbitClient) close(ctx context.Context) {
	log := NewLogFromCtx(ctx)
	if err := c.ch.Close(); err != nil {
		log.Error("", zap.Error(err))
	}
}

func (rmq *rabbitService) newSubClient(ctx context.Context) (*rabbitClient, error) {
	return rmq.newClient(ctx, "sub", rmq.coSub.Channel)
}

func (rmq *rabbitService) newPubClient(ctx context.Context) (*rabbitClient, error) {
	return rmq.newClient(ctx, "pub", rmq.coPub.Channel)
}

type channelFunc func() (*amqp.Channel, error)

func (rmq *rabbitService) newClient(ctx context.Context, kind string, coCh channelFunc) (c *rabbitClient, err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()

	// TODO: use github.com/devimteam/amqp
	// Cut off here if client ever canceled
	if err = ctx.Err(); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	log.Debug("acquiring channel", zap.String("kind", kind))
	var ch *amqp.Channel
	if ch, err = coCh(); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("acquired channel",
		zap.String("kind", kind),
		zap.Duration("in", time.Since(start)),
	)

	if err = ch.Confirm(false); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	c = &rabbitClient{
		ch:       ch,
		confirms: make(chan amqp.Confirmation, 1),
	}
	c.ch.NotifyPublish(c.confirms)
	log.Debug("new client",
		zap.String("kind", kind),
		zap.Duration("in", time.Since(start)),
	)
	return
}

func (c *rabbitClient) publish(ctx context.Context, key string, pb proto.Message) (err error) {
	log := NewLogFromCtx(ctx)

	var payload []byte
	bytes := -1
	{
		log.Debug("encoding", zap.Reflect("pb", pb))
		start := time.Now()
		if payload, err = proto.Marshal(pb); err != nil {
			log.Error("", zap.Error(err))
			return
		}
		bytes = len(payload)
		log.Debug("encoded", zap.Int("bytes", bytes),
			zap.Duration("in", time.Since(start)))
	}

	log.Debug("publishing", zap.String("rk", key))
	start := time.Now()
	if err = c.ch.Publish(
		rabbitExchange,
		key,
		false, // mandatory
		false, // immediate
		amqp.Publishing{
			Body:         payload,
			Expiration:   rabbitMessageExpirationS,
			DeliveryMode: amqp.Transient,
			AppId:        rabbitApp,
		},
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	// Starts from 1
	c.deliveryTag++
	dtag := c.deliveryTag
	log.Debug("published", zap.Int("bytes", bytes),
		zap.Uint64("dtag", dtag),
		zap.Duration("in", time.Since(start)))

	select {
	case confirmed := <-c.confirms:
		if got := confirmed.DeliveryTag; dtag != got {
			err = fmt.Errorf("expected DeliveryTag=%d, got %d", dtag, got)
			log.Error("", zap.Error(err))
			return
		}

		if !confirmed.Ack {
			err = fmt.Errorf("rabbitClient nack-ed message #%d", dtag)
			log.Error("", zap.Error(err))
			return
		}

		log.Debug("ack-ed msg", zap.Uint64("dtag", dtag),
			zap.Duration("whole publish in", time.Since(start)))
		return

	case <-time.After(rabbitAckTimeout):
	}
	err = fmt.Errorf("no ack from %s to #%d after %v", key, dtag, rabbitAckTimeout)
	log.Error("", zap.Error(err))
	return
}

func (c *rabbitClient) qDeclare(ctx context.Context, q string) (r amqp.Queue, err error) {
	log := NewLogFromCtx(ctx)
	args := amqp.Table{"x-message-ttl": rabbitMessageExpiration}
	if err = args.Validate(); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	start := time.Now()
	if r, err = c.ch.QueueDeclare(
		q,     // name of the queue
		false, // durable
		true,  // delete when unused
		false, // exclusive
		false, // noWait
		args,  // arguments
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("declared", zap.String("q", q),
		zap.Int("messages", r.Messages),
		zap.Int("consumers", r.Consumers),
		zap.Duration("in", time.Since(start)))
	return
}

func (c *rabbitClient) qBind(ctx context.Context, q, key string) (err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()
	if err = c.ch.QueueBind(
		q,              // name of the queue
		key,            // bindingKey
		rabbitExchange, // sourceExchange
		false,          // noWait
		nil,            // arguments
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("bound", zap.String("bk", key),
		zap.String("q", q),
		zap.Duration("in", time.Since(start)))
	return
}

func (c *rabbitClient) qUnbind(ctx context.Context, q, key string) (err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()
	if err = c.ch.QueueUnbind(
		q,              // name of the queue
		key,            // bindingKey
		rabbitExchange, // sourceExchange
		nil,            // args
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("unbound", zap.String("bk", key),
		zap.String("q", q),
		zap.Duration("in", time.Since(start)))
	return
}

func (c *rabbitClient) qDelete(ctx context.Context, q string) (purged int, err error) {
	log := NewLogFromCtx(ctx)
	start := time.Now()
	if purged, err = c.ch.QueueDelete(
		q,     // name of the queue
		false, // ifUnused
		false, // ifEmpty
		false, // noWait
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("deleted", zap.String("q", q),
		zap.Int("purged", purged),
		zap.Duration("in", time.Since(start)))
	return
}

func (c *rabbitClient) qConsume(ctx context.Context, q string) (
	deliveries <-chan amqp.Delivery,
	cancel func(),
	err error,
) {
	log := NewLogFromCtx(ctx)
	// Assumes queue name is unique enough on this channel
	// to be used as a consumer tag.
	cancel = func() {
		if err := c.ch.Cancel(q, true); err != nil {
			log.Error("", zap.Error(err))
		}
	}
	start := time.Now()
	if deliveries, err = c.ch.Consume(
		q,     // name
		q,     // consumerTag,
		false, // noAck
		false, // exclusive
		false, // noLocal
		false, // noWait
		nil,   // arguments
	); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Debug("started consuming", zap.String("q", q),
		zap.Duration("in", time.Since(start)))
	return
}
