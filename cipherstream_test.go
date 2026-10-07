package cipherstream

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestServerRenderedPages(t *testing.T) {
	tests := []struct {
		name string
		data any
		want []string
	}{
		{
			name: "home",
			data: homeData{Videos: []*Video{}, MaxUploadLabel: "4.0 GB"},
			want: []string{"Your video,", "multipart/form-data", "Upload &amp; convert"},
		},
		{
			name: "watch page",
			data: watchData{Video: &Video{ID: "0123456789abcdef01234567", Name: "sample.mp4", Status: "ready", CreatedAt: time.Now(), SizeBytes: 100}, StreamURL: "/hls/0123456789abcdef01234567/index.m3u8"},
			want: []string{"sample.mp4", "id=\"hls-player\"", "data-hls-src=\"/hls/0123456789abcdef01234567/index.m3u8\"", "new Hls("},
		},
		{
			name: "missing video",
			data: watchData{Missing: true},
			want: []string{"This video link is unavailable."},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := page.ExecuteTemplate(&output, "index.html", tt.data); err != nil {
				t.Fatalf("render failed: %v", err)
			}
			for _, text := range tt.want {
				if !strings.Contains(output.String(), text) {
					t.Errorf("rendered HTML does not contain %q", text)
				}
			}
		})
	}
}

func TestSafeExtension(t *testing.T) {
	for input, want := range map[string]string{
		".MP4":               ".mp4",
		".mkv":               ".mkv",
		"":                   ".video",
		"../../oops":         ".video",
		".verylongextension": ".video",
	} {
		if got := safeExtension(input); got != want {
			t.Errorf("safeExtension(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestValidID(t *testing.T) {
	if !validID("0123456789abcdef01234567") {
		t.Fatal("expected valid video ID")
	}
	for _, id := range []string{"", "../0123456789abcdef01234567", "0123456789abcdef0123456g"} {
		if validID(id) {
			t.Errorf("accepted invalid ID %q", id)
		}
	}
}

func TestVideoEncoderArgs(t *testing.T) {
	tests := []struct {
		encoder string
		want    []string
	}{
		{encoder: "libx264", want: []string{"-c:v", "libx264", "-crf", "23", "-preset", "veryfast"}},
		{encoder: "h264_nvenc", want: []string{"-c:v", "h264_nvenc", "-cq:v", "23", "-preset", "p4"}},
		{encoder: "h264_qsv", want: []string{"-c:v", "h264_qsv", "-global_quality", "23", "-preset", "veryfast"}},
	}
	for _, tt := range tests {
		got := strings.Join(videoEncoderArgs(tt.encoder, 23), " ")
		if want := strings.Join(tt.want, " "); got != want {
			t.Errorf("videoEncoderArgs(%q) = %q, want %q", tt.encoder, got, want)
		}
	}
}

func TestReusablePackageUploadAndPersistence(t *testing.T) {
	dataDir := t.TempDir()
	config := Config{DataDir: dataDir, FFmpegPath: "/bin/false", FFprobePath: "/bin/false", Workers: 1}
	service, err := New(config)
	if err != nil {
		t.Fatal(err)
	}
	video, err := service.Upload(context.Background(), "sample.mp4", strings.NewReader("not a real video"), 23)
	if err != nil {
		t.Fatal(err)
	}
	if video.ID == "" || video.Name != "sample.mp4" {
		t.Fatalf("unexpected upload result: %#v", video)
	}
	if err := service.Close(); err != nil {
		t.Fatal(err)
	}
	reopened, err := New(config)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	loaded, ok := reopened.GetVideo(video.ID)
	if !ok || loaded.sourceFile == "" {
		t.Fatalf("video metadata/source path did not reload: %#v, found=%v", loaded, ok)
	}
	if _, err := os.Stat(filepath.Join(dataDir, "uploads", loaded.sourceFile)); err != nil {
		t.Fatalf("stored input missing after restart: %v", err)
	}
	encoded, err := json.Marshal(loaded)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encoded), "source_file") {
		t.Fatal("private source path leaked through the public Video type")
	}
}

func TestHLSHandlerAndJSONAPIs(t *testing.T) {
	service, err := New(Config{DataDir: t.TempDir(), FFmpegPath: "/bin/false", FFprobePath: "/bin/false", Workers: 1})
	if err != nil {
		t.Fatal(err)
	}
	defer service.Close()
	id := "0123456789abcdef01234567"
	videoDir := filepath.Join(service.videoDir, id)
	if err := os.MkdirAll(videoDir, 0o755); err != nil {
		t.Fatal(err)
	}
	playlist := "#EXTM3U\n#EXT-X-ENDLIST\n"
	if err := os.WriteFile(filepath.Join(videoDir, "index.m3u8"), []byte(playlist), 0o644); err != nil {
		t.Fatal(err)
	}
	service.mu.Lock()
	service.videos[id] = &Video{ID: id, Name: "sample.mp4", Status: "ready", CreatedAt: time.Now(), sourceFile: "private.mp4"}
	service.mu.Unlock()

	handler := service.Handler()
	for _, path := range []string{"/hls/" + id + "/index.m3u8", "/api/videos/" + id, "/hls.js"} {
		recorder := httptest.NewRecorder()
		handler.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, path, nil))
		if recorder.Code != http.StatusOK {
			t.Errorf("GET %s returned %d: %s", path, recorder.Code, recorder.Body.String())
		}
		if path == "/hls/"+id+"/index.m3u8" && recorder.Body.String() != playlist {
			t.Errorf("unexpected playlist body %q", recorder.Body.String())
		}
		if path == "/hls.js" && recorder.Body.Len() < 500000 {
			t.Errorf("bundled HLS.js asset looks incomplete: %d bytes", recorder.Body.Len())
		}
		if path == "/api/videos/"+id && !strings.Contains(recorder.Body.String(), `"playlist_url":"/hls/`+id) {
			t.Errorf("API response did not include playlist URL: %s", recorder.Body.String())
		}
	}

	mux := http.NewServeMux()
	mux.Handle("GET /media/", http.StripPrefix("/media/", service.HLSHandler()))
	recorder := httptest.NewRecorder()
	mux.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "/media/"+id+"/index.m3u8", nil))
	if recorder.Code != http.StatusOK || recorder.Body.String() != playlist {
		t.Fatalf("custom mounted HLS handler failed: status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}
