package main

import (
	"bytes"
	"context"
	_ "embed"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/signal"
	"time"

	"github.com/fenollp/reMarkable-tools/whiteboard-server/hypercards"
	"github.com/gorilla/mux"
	"go.uber.org/zap"
)

//go:embed nothing_to_see_here.png
var defaultPNG []byte

//go:embed screensharing_embedding_room.html
var indexHTML string

func init() {
	hypercards.MustSetupLogging()
}

func main() {
	ctx := context.Background()
	log := hypercards.NewLogFromCtx(ctx)

	srv, err := hypercards.NewServer(ctx, true)
	if err != nil {
		panic(err)
	}

	port := ":" + os.Getenv("PORT")
	log.Info("starting HTTP server", zap.String("port", port))

	router := mux.NewRouter()
	sr := router.PathPrefix(os.Getenv("PATH_PREFIX")).Subrouter()

	// HTML page embedding image
	sr.HandleFunc("/{roomID}/", func(w http.ResponseWriter, r *http.Request) {
		log.Info(logReq(r))
		log.Info("rendering page", zap.Any("vars", mux.Vars(r)))
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		fmt.Fprint(w, indexHTML)
	})

	// Image
	sr.HandleFunc("/{roomID}/s.png", func(w http.ResponseWriter, r *http.Request) {
		log.Info(logReq(r))
		vars := mux.Vars(r)
		roomID := vars["roomID"]
		log.Info("rendering image", zap.String("roomID", roomID))
		rep, err := srv.RecvScreen(r.Context(), &hypercards.RecvScreenReq{RoomId: roomID})
		if err != nil {
			log.Error("", zap.Error(err))
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		w.Header().Set("Content-Type", "image/png; charset=utf-8")
		// From https://stackoverflow.com/a/2068407/1418165
		w.Header().Set("Cache-Control", "no-store, must-revalidate")
		w.Header().Set("Pragma", "no-cache")
		w.Header().Set("Expires", "0")
		data := rep.GetCanvasPng()
		if len(data) == 0 {
			data = defaultPNG
		}
		io.CopyN(w, bytes.NewReader(data), int64(len(data)))
	})

	hrv := &http.Server{
		Addr:         port,
		WriteTimeout: time.Second * 15,
		ReadTimeout:  time.Second * 15,
		IdleTimeout:  time.Second * 60,
		Handler:      router,
	}
	go func() {
		if err := hrv.ListenAndServe(); err != nil {
			log.Error("", zap.Error(err))
		}
	}()
	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt)

	<-c

	ctx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	hrv.Shutdown(ctx)
	log.Info("shutting down")
	os.Exit(0)
}

func logReq(r *http.Request) string {
	return fmt.Sprintf(
		"%s %s %q %q",
		r.Method,
		r.URL.String(),
		r.Header.Get("Referer"),
		r.Header.Get("User-Agent"),
	)
}
