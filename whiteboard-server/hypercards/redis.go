package hypercards

import (
	"context"
	"time"

	"github.com/go-redis/redis/v8"
	"go.uber.org/zap"
)

const (
	neverExpire   = 0
	defaultExpire = 24 * time.Hour
	redisNil      = redis.Nil
)

type redisClient struct {
	*redis.Client
}

func (srv *Server) red(ctx context.Context) *redisClient {
	return &redisClient{srv.rc.WithContext(ctx)}
}

func (srv *Server) setupRedis(ctx context.Context, host, port string) (err error) {
	log := NewLogFromCtx(ctx)
	redisHost := host + ":" + port
	log.Info("connecting to redis", zap.String("host", redisHost))
	srv.rc = &redisClient{redis.NewClient(&redis.Options{
		Addr:     redisHost,
		Password: "", // no password set
		DB:       0,  // use default DB
	})}
	start := time.Now()
	if _, err = srv.red(ctx).Ping(ctx).Result(); err != nil {
		log.Error("", zap.Error(err))
		return
	}
	log.Info("connected to redis", zap.Duration("in", time.Since(start)))
	return
}
