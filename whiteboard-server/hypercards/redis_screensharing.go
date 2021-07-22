package hypercards

import (
	"context"
	"time"

	"go.uber.org/zap"
)

func cacheKeyForScreenSharing(someID string) string { return "ScreenSharing-" + someID }

func (red *redisClient) getScreenSharing(ctx context.Context, someID string) (png []byte, err error) {
	log := NewLogFromCtx(ctx)
	beg := time.Now()
	key := cacheKeyForScreenSharing(someID)
	log.Debug("redis.Get",
		zap.String("key", key),
	)

	var data string
	if data, err = red.Get(ctx, key).Result(); err != nil && err != redisNil {
		log.Error("", zap.Error(err))
		return
	}
	if err == redisNil {
		err = nil
	}
	png = []byte(data)

	log.Info("redis.Get",
		zap.String("key", key),
		zap.Int("len(value)", len(png)),
		zap.Duration("in", time.Since(beg)),
	)
	return
}

func (red *redisClient) setScreenSharing(ctx context.Context, someID string, png []byte) (err error) {
	log := NewLogFromCtx(ctx)
	beg := time.Now()
	key := cacheKeyForScreenSharing(someID)
	exp := defaultExpire
	log.Debug("redis.Set",
		zap.String("key", key),
		zap.Int("len(value)", len(png)),
		zap.Duration("expire", exp),
	)

	if err = red.Set(ctx, key, png, exp).Err(); err != nil {
		log.Error("", zap.Error(err))
		return
	}

	log.Info("redis.Set",
		zap.String("key", key),
		zap.Int("len(value)", len(png)),
		zap.Duration("expire", exp),
		zap.Duration("in", time.Since(beg)),
	)
	return
}
