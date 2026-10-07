package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/coffeeinit/cipherstream"
)

func main() {
	service, err := cipherstream.New(cipherstream.Config{
		DataDir:        env("DATA_DIR", "data"),
		FFmpegPath:     env("FFMPEG_BIN", "ffmpeg"),
		FFprobePath:    os.Getenv("FFPROBE_BIN"),
		VideoEncoder:   env("FFMPEG_VIDEO_ENCODER", "libx264"),
		MaxUploadBytes: envInt64("MAX_UPLOAD_BYTES", 4<<30),
		Workers:        2,
	})
	if err != nil {
		log.Fatal(err)
	}
	defer service.Close()

	addr := env("PORT", "8080")
	if !strings.Contains(addr, ":") {
		addr = ":" + addr
	}
	httpServer := &http.Server{Addr: addr, Handler: service.Handler(), ReadHeaderTimeout: 10 * time.Second}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		_ = httpServer.Shutdown(shutdownCtx)
	}()

	log.Printf("CipherStream demo listening on %s", addr)
	if err := httpServer.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}

func env(key, fallback string) string {
	if value := strings.TrimSpace(os.Getenv(key)); value != "" {
		return value
	}
	return fallback
}

func envInt64(key string, fallback int64) int64 {
	if value, err := strconv.ParseInt(os.Getenv(key), 10, 64); err == nil && value > 0 {
		return value
	}
	return fallback
}
