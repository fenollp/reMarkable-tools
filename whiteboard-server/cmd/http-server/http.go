package main

import (
	"bytes"
	"context"
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

func init() {
	hypercards.MustSetupLogging()
}

const base = "/reMarkable-tools/ScreenSharing"

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

	// HTML page embedding image
	router.HandleFunc(base+"/{roomID}/", func(w http.ResponseWriter, r *http.Request) {
		log.Info(logReq(r))
		log.Info("rendering page", zap.Any("vars", mux.Vars(r)))
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		fmt.Fprint(w, index)
	})

	// Image
	router.HandleFunc(base+"/{roomID}/image.png", func(w http.ResponseWriter, r *http.Request) {
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
		w.Header().Set("Cache-Control", "no-store, must-revalidate")
		w.Header().Set("Pragma", "no-cache")
		w.Header().Set("Expires", "0")
		data := rep.GetPngCanvas()
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

const index = `
<!DOCTYPE html>
<html lang="en" data-layout="responsive">
	<head>
		<meta charset="utf-8">
		<meta http-equiv="X-UA-Compatible" content="IE=edge">
		<meta name="viewport" content="width=device-width, initial-scale=1.0">
		<title>reMarkable-tools · Live View HyperCard</title>
		<style type="text/css">
			#view {
			    max-width: 100%;
			    max-height: 100%;
			    bottom: 0;
			    left: 0;
			    margin: auto;
			    overflow: auto;
			    position: fixed;
			    right: 0;
			    top: 0;
			    -o-object-fit: contain;
			    object-fit: contain;
			}
		</style>
		<script type="text/javascript">
			setInterval(function() {
				var node = document.getElementById('view');
				node.src = './image.png';
			}, 1000);
		</script>
	</head>
	<body>
		<div><img id="view" src="./image.png" alt="reMarkable whiteboard screen"/></div>
		<!-- <br/><p>Find out more <a href="https://github.com/fenollp/reMarkable-tools">on GitHub</a></p> -->
	</body>
</html>
`

func logReq(r *http.Request) string {
	return fmt.Sprintf(
		"%s %s %q %q",
		r.Method,
		r.URL.String(),
		r.Header.Get("Referer"),
		r.Header.Get("User-Agent"),
	)
}
