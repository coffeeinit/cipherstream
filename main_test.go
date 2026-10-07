package main

import (
	"bytes"
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
			data: watchData{Video: &Video{ID: "0123456789abcdef01234567", Name: "sample.mp4", Status: "ready", CreatedAt: time.Now(), SizeBytes: 100}, StreamURL: "/stream/0123456789abcdef01234567/index.m3u8"},
			want: []string{"sample.mp4", "<video", "/stream/0123456789abcdef01234567/index.m3u8"},
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
