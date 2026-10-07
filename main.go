package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/rand"
	"embed"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"io"
	"log"
	"mime"
	"mime/multipart"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

//go:embed web/*
var webFiles embed.FS

var page = template.Must(template.New("index.html").Funcs(template.FuncMap{
	"bytes":       formatBytes,
	"duration":    formatDuration,
	"time":        func(t time.Time) string { return t.Local().Format("Jan 2, 2006 · 3:04 PM") },
	"statusLabel": func(s string) string { return strings.ToUpper(s) },
}).ParseFS(webFiles, "web/index.html"))

const defaultMaxUpload = int64(4 << 30)

type Video struct {
	ID              string    `json:"id"`
	Name            string    `json:"name"`
	Status          string    `json:"status"`
	Progress        int       `json:"progress"`
	Phase           string    `json:"phase,omitempty"`
	Error           string    `json:"error,omitempty"`
	CreatedAt       time.Time `json:"created_at"`
	DurationSeconds float64   `json:"duration_seconds,omitempty"`
	SizeBytes       int64     `json:"size_bytes"`
	CRF             int       `json:"crf"`
	Remuxed         bool      `json:"remuxed"`
	SourceFile      string    `json:"source_file,omitempty"`
}

type probeResult struct {
	Format struct {
		Duration string `json:"duration"`
	} `json:"format"`
	Streams []struct {
		CodecType string `json:"codec_type"`
		CodecName string `json:"codec_name"`
	} `json:"streams"`
}

type App struct {
	dataDir     string
	uploadDir   string
	videoDir    string
	ffmpeg      string
	ffprobe     string
	encoder     string
	maxUpload   int64
	videos      map[string]*Video
	mu          sync.RWMutex
	jobs        chan string
	workerCount int
}

type homeData struct {
	Video          *Video
	Missing        bool
	Videos         []*Video
	Pending        bool
	Notice         string
	Error          string
	MaxUploadLabel string
}

type watchData struct {
	Video     *Video
	Missing   bool
	StreamURL string
}

func main() {
	dataDir := envString("DATA_DIR", "data")
	ffmpeg := envString("FFMPEG_BIN", "ffmpeg")
	app, err := newApp(dataDir, ffmpeg, envString("FFMPEG_VIDEO_ENCODER", "libx264"), envInt64("MAX_UPLOAD_BYTES", defaultMaxUpload), 2)
	if err != nil {
		log.Fatal(err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	app.startWorkers(ctx)

	mux := http.NewServeMux()
	mux.HandleFunc("GET /", app.handleHome)
	mux.HandleFunc("POST /upload", app.handleUpload)
	mux.HandleFunc("GET /watch/{id}", app.handleWatch)
	mux.HandleFunc("GET /stream/{id}/{file...}", app.handleStream)
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte("ok\n"))
	})
	mux.HandleFunc("GET /style.css", app.handleStyle)

	addr := envString("PORT", "8080")
	if !strings.Contains(addr, ":") {
		addr = ":" + addr
	}
	server := &http.Server{Addr: addr, Handler: securityHeaders(mux), ReadHeaderTimeout: 10 * time.Second}
	log.Printf("CipherStream listening on %s (local data: %s)", addr, dataDir)
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatal(err)
	}
}

func newApp(dataDir, ffmpeg, encoder string, maxUpload int64, workers int) (*App, error) {
	if maxUpload < 1 {
		maxUpload = defaultMaxUpload
	}
	if workers < 1 {
		workers = 1
	}
	uploadDir := filepath.Join(dataDir, "uploads")
	videoDir := filepath.Join(dataDir, "videos")
	for _, dir := range []string{uploadDir, videoDir} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, fmt.Errorf("create data directory %s: %w", dir, err)
		}
	}
	a := &App{
		dataDir: dataDir, uploadDir: uploadDir, videoDir: videoDir,
		ffmpeg: ffmpeg, ffprobe: siblingBinary(ffmpeg, "ffprobe"), encoder: encoder,
		maxUpload: maxUpload, videos: make(map[string]*Video), jobs: make(chan string, 256), workerCount: workers,
	}
	if err := a.loadVideos(); err != nil {
		return nil, err
	}
	return a, nil
}

func siblingBinary(binary, sibling string) string {
	if strings.ContainsRune(binary, filepath.Separator) {
		return filepath.Join(filepath.Dir(binary), sibling)
	}
	return sibling
}

func (a *App) loadVideos() error {
	entries, err := os.ReadDir(a.videoDir)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if !entry.IsDir() || !validID(entry.Name()) {
			continue
		}
		data, err := os.ReadFile(filepath.Join(a.videoDir, entry.Name(), "video.json"))
		if err != nil {
			continue
		}
		var video Video
		if json.Unmarshal(data, &video) != nil || video.ID != entry.Name() {
			continue
		}
		if video.Status == "processing" || video.Status == "queued" {
			video.Status, video.Progress, video.Phase = "queued", 0, "Waiting to convert"
		}
		a.videos[video.ID] = &video
		if video.Status == "queued" {
			a.jobs <- video.ID
		}
	}
	return nil
}

func (a *App) startWorkers(ctx context.Context) {
	for i := 0; i < a.workerCount; i++ {
		go func() {
			for {
				select {
				case <-ctx.Done():
					return
				case id := <-a.jobs:
					a.processVideo(ctx, id)
				}
			}
		}()
	}
}

func (a *App) processVideo(ctx context.Context, id string) {
	video := a.getVideo(id)
	if video == nil {
		return
	}
	input := filepath.Join(a.uploadDir, filepath.Base(video.SourceFile))
	output := filepath.Join(a.videoDir, id)
	if err := os.MkdirAll(output, 0o755); err != nil {
		a.failVideo(id, err)
		return
	}

	a.updateVideo(id, func(v *Video) { v.Status, v.Progress, v.Phase, v.Error = "processing", 1, "Inspecting video", "" })
	info, err := a.probe(ctx, input)
	if err != nil {
		a.failVideo(id, fmt.Errorf("could not read video: %w", err))
		return
	}
	if info.Format.Duration != "" {
		if duration, parseErr := strconv.ParseFloat(info.Format.Duration, 64); parseErr == nil && duration > 0 {
			a.updateVideo(id, func(v *Video) { v.DurationSeconds = duration })
		}
	}
	video = a.getVideo(id)
	if video == nil {
		return
	}
	hasVideo, hasH264, hasAAC := false, false, false
	for _, stream := range info.Streams {
		if stream.CodecType == "video" {
			hasVideo = true
			if stream.CodecName == "h264" {
				hasH264 = true
			}
		}
		if stream.CodecType == "audio" && stream.CodecName == "aac" {
			hasAAC = true
		}
	}
	if !hasVideo {
		a.failVideo(id, errors.New("the uploaded file does not contain a video stream"))
		return
	}
	remux := hasH264 && hasAAC
	a.updateVideo(id, func(v *Video) {
		v.Remuxed = remux
		if remux {
			v.Phase = "Packaging HLS (no re-encode needed)"
		} else {
			v.Phase = "Converting to H.264 + AAC"
		}
	})
	if err := a.transcode(ctx, id, input, output, video.CRF, video.DurationSeconds, remux); err != nil {
		a.failVideo(id, err)
		return
	}
	a.makeThumbnail(ctx, input, output, video.DurationSeconds)
	a.updateVideo(id, func(v *Video) { v.Status, v.Progress, v.Phase, v.Error = "ready", 100, "Ready to watch", "" })
}

func (a *App) probe(ctx context.Context, input string) (probeResult, error) {
	var result probeResult
	cmd := exec.CommandContext(ctx, a.ffprobe, "-v", "error", "-show_entries", "format=duration:stream=codec_type,codec_name", "-of", "json", input)
	out, err := cmd.Output()
	if err != nil {
		return result, commandError("ffprobe", err)
	}
	if err := json.Unmarshal(out, &result); err != nil {
		return result, err
	}
	if len(result.Streams) == 0 {
		return result, errors.New("no readable audio or video streams found")
	}
	return result, nil
}

func (a *App) transcode(ctx context.Context, id, input, output string, crf int, duration float64, remux bool) error {
	playlist := filepath.Join(output, "index.m3u8")
	segments := filepath.Join(output, "segment_%05d.ts")
	args := []string{"-hide_banner", "-y", "-i", input, "-map", "0:v:0", "-map", "0:a:0?"}
	if remux {
		args = append(args, "-c:v", "copy", "-c:a", "copy")
	} else {
		args = append(args, videoEncoderArgs(a.encoder, crf)...)
		args = append(args, "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-ac", "2")
	}
	args = append(args, "-hls_time", "6", "-hls_playlist_type", "vod", "-hls_flags", "independent_segments", "-hls_segment_filename", segments, "-progress", "pipe:1", "-nostats", playlist)
	cmd := exec.CommandContext(ctx, a.ffmpeg, args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return commandError("ffmpeg", err)
	}
	tail := &tailWriter{limit: 8192}
	var wg sync.WaitGroup
	wg.Add(2)
	go func() { defer wg.Done(); _, _ = io.Copy(tail, stderr) }()
	go func() {
		defer wg.Done()
		scanner := bufio.NewScanner(stdout)
		scanner.Buffer(make([]byte, 4096), 1024*1024)
		var outTime float64
		lastUpdate := time.Time{}
		for scanner.Scan() {
			line := scanner.Text()
			if strings.HasPrefix(line, "out_time_ms=") {
				if value, parseErr := strconv.ParseFloat(strings.TrimPrefix(line, "out_time_ms="), 64); parseErr == nil {
					outTime = value / 1_000_000
				}
			}
			if strings.HasPrefix(line, "progress=") && duration > 0 && time.Since(lastUpdate) > 650*time.Millisecond {
				progress := 3 + int((outTime/duration)*92)
				if progress > 95 {
					progress = 95
				}
				if progress < 3 {
					progress = 3
				}
				a.updateVideo(id, func(v *Video) { v.Progress = progress })
				lastUpdate = time.Now()
			}
		}
	}()
	waitErr := cmd.Wait()
	wg.Wait()
	if waitErr != nil {
		message := strings.TrimSpace(tail.String())
		if message == "" {
			message = waitErr.Error()
		}
		if len(message) > 1200 {
			message = message[len(message)-1200:]
		}
		return fmt.Errorf("FFmpeg conversion failed: %s", message)
	}
	return nil
}

func videoEncoderArgs(encoder string, crf int) []string {
	switch strings.ToLower(encoder) {
	case "h264_nvenc":
		return []string{"-c:v", encoder, "-cq:v", strconv.Itoa(crf), "-preset", "p4"}
	case "h264_qsv":
		return []string{"-c:v", encoder, "-global_quality", strconv.Itoa(crf), "-preset", "veryfast"}
	default:
		return []string{"-c:v", encoder, "-crf", strconv.Itoa(crf), "-preset", "veryfast"}
	}
}

func (a *App) makeThumbnail(ctx context.Context, input, output string, duration float64) {
	at := duration * 0.25
	if at < 0 {
		at = 0
	}
	cmd := exec.CommandContext(ctx, a.ffmpeg, "-hide_banner", "-loglevel", "error", "-ss", fmt.Sprintf("%.3f", at), "-i", input, "-frames:v", "1", "-vf", "scale=640:-2", "-q:v", "4", "-y", filepath.Join(output, "thumbnail.jpg"))
	if err := cmd.Run(); err != nil {
		log.Printf("thumbnail extraction skipped: %v", err)
	}
}

func (a *App) failVideo(id string, err error) {
	a.updateVideo(id, func(v *Video) { v.Status, v.Phase, v.Error = "failed", "Conversion failed", err.Error() })
	log.Printf("video %s failed: %v", id, err)
}

func (a *App) handleHome(w http.ResponseWriter, r *http.Request) {
	data := homeData{Videos: a.listVideos(), MaxUploadLabel: formatBytes(a.maxUpload)}
	for _, video := range data.Videos {
		if video.Status == "queued" || video.Status == "processing" {
			data.Pending = true
		}
	}
	if r.URL.Query().Get("uploaded") != "" {
		count, _ := strconv.Atoi(r.URL.Query().Get("uploaded"))
		if count == 1 {
			data.Notice = "Upload received — your video is being prepared."
		} else if count > 1 {
			data.Notice = fmt.Sprintf("%d uploads received — your videos are being prepared.", count)
		}
	}
	a.render(w, http.StatusOK, data)
}

func (a *App) handleUpload(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, a.maxUpload+(1<<20))
	reader, err := r.MultipartReader()
	if err != nil {
		a.renderUploadError(w, "Choose one or more video files to upload.")
		return
	}
	crf := 23
	uploaded := 0
	var uploadedIDs []string
	for {
		part, nextErr := reader.NextPart()
		if errors.Is(nextErr, io.EOF) {
			break
		}
		if nextErr != nil {
			a.queueVideos(uploadedIDs, crf)
			if uploaded > 0 {
				a.renderUploadError(w, fmt.Sprintf("%d video(s) were accepted, but the rest of the upload could not be read: %s", uploaded, uploadError(nextErr)))
			} else {
				a.renderUploadError(w, uploadError(nextErr))
			}
			return
		}
		if part.FormName() == "crf" {
			value, _ := io.ReadAll(io.LimitReader(part, 32))
			if parsed, parseErr := strconv.Atoi(strings.TrimSpace(string(value))); parseErr == nil && parsed >= 18 && parsed <= 32 {
				crf = parsed
			}
			part.Close()
			continue
		}
		if part.FormName() != "file" || part.FileName() == "" {
			part.Close()
			continue
		}
		video, err := a.savePart(part, crf)
		if err != nil {
			part.Close()
			a.queueVideos(uploadedIDs, crf)
			message := uploadError(err)
			if uploaded > 0 {
				message = fmt.Sprintf("%d video(s) were accepted. Another file could not be saved: %s", uploaded, message)
			}
			a.renderUploadError(w, message)
			return
		}
		part.Close()
		uploadedIDs = append(uploadedIDs, video.ID)
		uploaded++
	}
	if uploaded == 0 {
		a.renderUploadError(w, "Choose one or more video files to upload.")
		return
	}
	a.queueVideos(uploadedIDs, crf)
	http.Redirect(w, r, "/?uploaded="+strconv.Itoa(uploaded), http.StatusSeeOther)
}

func (a *App) savePart(part *multipart.Part, crf int) (*Video, error) {
	id, err := newID()
	if err != nil {
		return nil, err
	}
	name := filepath.Base(strings.ReplaceAll(part.FileName(), "\\", "/"))
	if name == "." || name == "/" || name == "" {
		name = "video-upload"
	}
	storedName := id + safeExtension(filepath.Ext(name))
	dest := filepath.Join(a.uploadDir, storedName)
	out, err := os.OpenFile(dest, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if err != nil {
		return nil, err
	}
	size, copyErr := io.Copy(out, io.LimitReader(part, a.maxUpload+1))
	closeErr := out.Close()
	if copyErr != nil || closeErr != nil || size > a.maxUpload {
		_ = os.Remove(dest)
		if copyErr != nil {
			return nil, copyErr
		}
		if closeErr != nil {
			return nil, closeErr
		}
		return nil, errors.New("video exceeds the configured per-file upload limit")
	}
	video := &Video{ID: id, Name: name, Status: "queued", Progress: 0, Phase: "Waiting to convert", CreatedAt: time.Now().UTC(), SizeBytes: size, CRF: crf, SourceFile: storedName}
	a.mu.Lock()
	a.videos[id] = video
	persistErr := a.persistLocked(video)
	if persistErr != nil {
		delete(a.videos, id)
	}
	a.mu.Unlock()
	if persistErr != nil {
		_ = os.Remove(dest)
		return nil, persistErr
	}
	return video, nil
}

func (a *App) queueVideos(ids []string, crf int) {
	for _, id := range ids {
		a.updateVideo(id, func(video *Video) { video.CRF = crf })
		a.jobs <- id
	}
}

func (a *App) renderUploadError(w http.ResponseWriter, message string) {
	data := homeData{Videos: a.listVideos(), MaxUploadLabel: formatBytes(a.maxUpload), Error: message}
	for _, video := range data.Videos {
		if video.Status == "queued" || video.Status == "processing" {
			data.Pending = true
		}
	}
	a.render(w, http.StatusBadRequest, data)
}

func uploadError(err error) string {
	if strings.Contains(strings.ToLower(err.Error()), "request body too large") || strings.Contains(strings.ToLower(err.Error()), "limit") {
		return "Video exceeds the configured upload limit."
	}
	return "The upload could not be read. Check the file and try again."
}

func (a *App) handleWatch(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	video := a.getVideo(id)
	if video == nil {
		a.render(w, http.StatusNotFound, watchData{Missing: true})
		return
	}
	if video.Status == "queued" || video.Status == "processing" {
		w.Header().Set("Refresh", "3")
	}
	a.render(w, http.StatusOK, watchData{Video: video, StreamURL: "/stream/" + id + "/index.m3u8"})
}

func (a *App) handleStream(w http.ResponseWriter, r *http.Request) {
	id, file := r.PathValue("id"), r.PathValue("file")
	clean := filepath.Clean(filepath.FromSlash(file))
	if !validID(id) || clean == "." || filepath.IsAbs(clean) || strings.HasPrefix(clean, ".."+string(filepath.Separator)) || clean == ".." {
		http.NotFound(w, r)
		return
	}
	video := a.getVideo(id)
	if video == nil || video.Status != "ready" {
		http.NotFound(w, r)
		return
	}
	path := filepath.Join(a.videoDir, id, clean)
	info, err := os.Stat(path)
	if err != nil || info.IsDir() {
		http.NotFound(w, r)
		return
	}
	ext := strings.ToLower(filepath.Ext(path))
	contentType := mime.TypeByExtension(ext)
	switch ext {
	case ".m3u8":
		contentType = "application/vnd.apple.mpegurl"
	case ".ts":
		contentType = "video/mp2t"
	case ".jpg", ".jpeg":
		contentType = "image/jpeg"
	}
	if contentType != "" {
		w.Header().Set("Content-Type", contentType)
	}
	w.Header().Set("Cache-Control", "public, max-age=3600")
	http.ServeFile(w, r, path)
}

func (a *App) handleStyle(w http.ResponseWriter, r *http.Request) {
	data, err := webFiles.ReadFile("web/style.css")
	if err != nil {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("Content-Type", "text/css; charset=utf-8")
	w.Header().Set("Cache-Control", "public, max-age=86400")
	_, _ = w.Write(data)
}

func (a *App) render(w http.ResponseWriter, status int, data any) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	if err := page.ExecuteTemplate(w, "index.html", data); err != nil {
		log.Printf("render page: %v", err)
	}
}

func (a *App) listVideos() []*Video {
	a.mu.RLock()
	videos := make([]*Video, 0, len(a.videos))
	for _, video := range a.videos {
		copy := *video
		videos = append(videos, &copy)
	}
	a.mu.RUnlock()
	sort.Slice(videos, func(i, j int) bool { return videos[i].CreatedAt.After(videos[j].CreatedAt) })
	return videos
}

func (a *App) getVideo(id string) *Video {
	a.mu.RLock()
	defer a.mu.RUnlock()
	if video := a.videos[id]; video != nil {
		copy := *video
		return &copy
	}
	return nil
}

func (a *App) updateVideo(id string, update func(*Video)) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if video := a.videos[id]; video != nil {
		update(video)
		if err := a.persistLocked(video); err != nil {
			log.Printf("persist video %s: %v", id, err)
		}
	}
}

func (a *App) persistLocked(video *Video) error {
	dir := filepath.Join(a.videoDir, video.ID)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(video, "", "  ")
	if err != nil {
		return err
	}
	tmp := filepath.Join(dir, "video.json.tmp")
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, filepath.Join(dir, "video.json"))
}

func validID(id string) bool {
	if len(id) != 24 {
		return false
	}
	_, err := hex.DecodeString(id)
	return err == nil
}
func newID() (string, error) {
	var raw [12]byte
	if _, err := rand.Read(raw[:]); err != nil {
		return "", err
	}
	return hex.EncodeToString(raw[:]), nil
}
func safeExtension(ext string) string {
	ext = strings.ToLower(ext)
	if len(ext) > 12 || ext == "." {
		return ".video"
	}
	if ext == "" {
		return ".video"
	}
	for _, r := range ext {
		if !(r == '.' || r >= 'a' && r <= 'z' || r >= '0' && r <= '9') {
			return ".video"
		}
	}
	return ext
}
func formatBytes(n int64) string {
	if n < 0 {
		n = 0
	}
	units := []string{"B", "KB", "MB", "GB", "TB"}
	value := float64(n)
	i := 0
	for value >= 1024 && i < len(units)-1 {
		value /= 1024
		i++
	}
	if i == 0 {
		return fmt.Sprintf("%d %s", n, units[i])
	}
	return fmt.Sprintf("%.1f %s", value, units[i])
}
func formatDuration(s float64) string {
	n := int64(s)
	if n < 0 {
		n = 0
	}
	h := n / 3600
	m := (n % 3600) / 60
	sec := n % 60
	if h > 0 {
		return fmt.Sprintf("%d:%02d:%02d", h, m, sec)
	}
	return fmt.Sprintf("%d:%02d", m, sec)
}
func commandError(name string, err error) error {
	var execErr *exec.Error
	if errors.As(err, &execErr) {
		return fmt.Errorf("%s is unavailable on the server; install FFmpeg (ffmpeg and ffprobe) or run the Docker image", name)
	}
	return fmt.Errorf("%s: %w", name, err)
}
func envString(key, fallback string) string {
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
func securityHeaders(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("Referrer-Policy", "strict-origin-when-cross-origin")
		w.Header().Set("X-Frame-Options", "SAMEORIGIN")
		next.ServeHTTP(w, r)
	})
}

type tailWriter struct {
	buf   bytes.Buffer
	limit int
}

func (w *tailWriter) Write(p []byte) (int, error) {
	written := len(p)
	if len(p) >= w.limit {
		w.buf.Reset()
		_, _ = w.buf.Write(p[len(p)-w.limit:])
		return written, nil
	}
	if w.buf.Len()+len(p) > w.limit {
		current := w.buf.Bytes()
		keep := w.limit - len(p)
		tail := append([]byte(nil), current[len(current)-keep:]...)
		w.buf.Reset()
		_, _ = w.buf.Write(tail)
	}
	_, _ = w.buf.Write(p)
	return written, nil
}
func (w *tailWriter) String() string { return w.buf.String() }
