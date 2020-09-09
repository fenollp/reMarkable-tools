package main

import (
	"context"
	"fmt"
	"math/rand"
	"net"
	"os"
	"os/signal"
	"strconv"
	"syscall"

	wb "github.com/fenollp/reMarkable-tools/whiteboard-server/hypercard_whiteboard"
	"go.uber.org/zap"
	"google.golang.org/grpc"
)

var grpcPort = uint64(0)

func init() {
	port := os.Getenv("PORT")
	var err error
	if grpcPort, err = strconv.ParseUint(port, 10, 64); err != nil {
		panic(err)
	}

	wb.MustSetupLogging()
}

func main() {
	rand.Seed(87 + 66 + 83)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	log := wb.NewLogFromCtx(ctx)

	log.Info("starting runtime logic...")
	srv, err := wb.NewServer(ctx)
	if err != nil {
		log.Fatal("", zap.Error(err))
	}
	defer srv.Close(ctx)

	s := grpc.NewServer()
	defer s.Stop()
	wb.RegisterWhiteboardServer(s, srv)

	go func() {
		// Cuts ctx
		defer cancel()
		// Starves gRPC clients
		defer s.GracefulStop()

		die := make(chan os.Signal, 1)
		signal.Notify(die, os.Interrupt, syscall.SIGINT, syscall.SIGTERM)
		select {
		case sig := <-die:
			log.Info("dying", zap.String("sig", sig.String()))
		case <-ctx.Done():
			log.Info("background context DONE")
		}
	}()

	host := fmt.Sprintf(":%d", grpcPort)
	log.Info("listening on", zap.String("host", host))
	lis, err := net.Listen("tcp", host)
	if err != nil {
		log.Fatal("failed to listen", zap.Error(err))
	}
	defer lis.Close()

	if err = s.Serve(lis); err != nil {
		log.Fatal("failed to serve gRPC", zap.Error(err))
	}
}
