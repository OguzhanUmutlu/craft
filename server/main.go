package main

import (
	"context"
	"crypto/sha256"
	_ "embed"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

const AppVersion = "1.0.0"

//go:embed install.sh
var installShTemplate string

//go:embed install.ps1
var installPs1Template string

type ServerConfig struct {
	Port        string
	ReleasesDir string
	PublicURL   string
}

type Server struct {
	config    ServerConfig
	startTime time.Time
	mu        sync.RWMutex
	checksums map[string]string
}

func main() {
	var cfg ServerConfig
	flag.StringVar(&cfg.Port, "port", getEnv("PORT", "8080"), "Port to listen on")
	flag.StringVar(&cfg.ReleasesDir, "dir", getEnv("RELEASES_DIR", "./releases"), "Directory containing executable binaries")
	flag.StringVar(&cfg.PublicURL, "public-url", getEnv("PUBLIC_URL", ""), "Public URL base (e.g. https://dl.example.com)")
	flag.Parse()

	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	slog.SetDefault(logger)

	// Ensure releases directory exists
	if err := os.MkdirAll(cfg.ReleasesDir, 0755); err != nil {
		slog.Error("Failed to create releases directory", "error", err)
		os.Exit(1)
	}

	srv := &Server{
		config:    cfg,
		startTime: time.Now(),
		checksums: make(map[string]string),
	}
	srv.refreshChecksums()

	mux := http.NewServeMux()
	mux.HandleFunc("GET /", srv.handleRoot)
	mux.HandleFunc("GET /healthz", srv.handleHealthz)
	mux.HandleFunc("GET /api/v1/version", srv.handleVersion)
	mux.HandleFunc("GET /api/v1/download/{platform}", srv.handleDownload)
	mux.HandleFunc("GET /api/versions/ui", srv.handleUIVersions)
	mux.HandleFunc("GET /api/v1/versions/ui", srv.handleUIVersions)
	mux.HandleFunc("GET /download/ui", srv.handleDownloadUI)
	mux.HandleFunc("GET /download/ui/{platform}", srv.handleDownloadUIPlatform)
	mux.HandleFunc("GET /api/v1/download/ui/{platform}", srv.handleDownloadUIPlatform)
	mux.HandleFunc("GET /install.sh", srv.handleInstallSh)
	mux.HandleFunc("GET /install.ps1", srv.handleInstallPs1)

	// Wrap mux with CORS and Logging middleware
	handler := srv.corsMiddleware(srv.loggingMiddleware(mux))

	httpServer := &http.Server{
		Addr:         ":" + cfg.Port,
		Handler:      handler,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 300 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	// Graceful shutdown channel
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, os.Interrupt, syscall.SIGTERM)

	go func() {
		slog.Info("Craft Distribution Server starting", "port", cfg.Port, "releases_dir", cfg.ReleasesDir)
		if err := httpServer.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			slog.Error("Server error", "error", err)
			os.Exit(1)
		}
	}()

	<-stop
	slog.Info("Shutting down distribution server...")

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := httpServer.Shutdown(ctx); err != nil {
		slog.Error("Forced shutdown error", "error", err)
	}
	slog.Info("Server exited cleanly.")
}

func (s *Server) corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, OPTIONS, HEAD")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func (s *Server) loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		rw := &responseWriter{ResponseWriter: w, statusCode: http.StatusOK}
		next.ServeHTTP(rw, r)
		slog.Info("HTTP request",
			"method", r.Method,
			"path", r.URL.Path,
			"status", rw.statusCode,
			"duration", time.Since(start).String(),
			"remote", r.RemoteAddr,
		)
	})
}

type responseWriter struct {
	http.ResponseWriter
	statusCode int
}

func (rw *responseWriter) WriteHeader(code int) {
	rw.statusCode = code
	rw.ResponseWriter.WriteHeader(code)
}

func (s *Server) handleRoot(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}
	baseURL := s.getBaseURL(r)
	resp := map[string]any{
		"app":             "Craft Binary Distribution Server",
		"version":         AppVersion,
		"status":          "online",
		"uptime":          time.Since(s.startTime).String(),
		"docs_url":        "https://github.com/OguzhanUmutlu/craft",
		"install_sh":      baseURL + "/install.sh",
		"install_ps1":     baseURL + "/install.ps1",
		"api_version":     baseURL + "/api/v1/version",
		"download_ui":     baseURL + "/download/ui",
		"api_versions_ui": baseURL + "/api/versions/ui",
	}
	writeJSON(w, http.StatusOK, resp)
}

func (s *Server) handleHealthz(w http.ResponseWriter, r *http.Request) {
	resp := map[string]any{
		"status":         "healthy",
		"uptime_seconds": time.Since(s.startTime).Seconds(),
		"version":        AppVersion,
		"timestamp":      time.Now().UTC().Format(time.RFC3339),
	}
	writeJSON(w, http.StatusOK, resp)
}

func (s *Server) handleVersion(w http.ResponseWriter, r *http.Request) {
	baseURL := s.getBaseURL(r)
	s.mu.RLock()
	defer s.mu.RUnlock()

	platforms := []string{
		"craft-linux-amd64",
		"craft-linux-arm64",
		"craft-windows-amd64.exe",
		"craft-darwin-arm64",
		"craft-darwin-amd64",
	}

	assets := make(map[string]any)
	for _, p := range platforms {
		filePath := filepath.Join(s.config.ReleasesDir, p)
		info, err := os.Stat(filePath)
		var size int64 = 0
		available := false
		if err == nil && !info.IsDir() {
			size = info.Size()
			available = true
		}

		assets[p] = map[string]any{
			"url":       fmt.Sprintf("%s/api/v1/download/%s", baseURL, p),
			"size":      size,
			"available": available,
			"sha256":    s.checksums[p],
		}
	}

	resp := map[string]any{
		"version":      AppVersion,
		"release_date": "2026-09-11",
		"notes":        "Craft official production release - high-performance standalone Minecraft server toolchain and background daemon",
		"assets":       assets,
	}
	writeJSON(w, http.StatusOK, resp)
}

func (s *Server) handleDownload(w http.ResponseWriter, r *http.Request) {
	platform := r.PathValue("platform")
	if platform == "" {
		http.Error(w, "Platform parameter required", http.StatusBadRequest)
		return
	}

	cleanPlatform := filepath.Base(platform)
	targetFile := filepath.Join(s.config.ReleasesDir, cleanPlatform)

	// Fallback check: if platform requested is e.g. "linux-amd64" or "craft", check matches
	if _, err := os.Stat(targetFile); os.IsNotExist(err) {
		candidates := []string{
			"craft-" + cleanPlatform,
			"craft-" + cleanPlatform + ".exe",
			"craft",
		}
		found := false
		for _, cand := range candidates {
			candPath := filepath.Join(s.config.ReleasesDir, cand)
			if _, err := os.Stat(candPath); err == nil {
				targetFile = candPath
				cleanPlatform = cand
				found = true
				break
			}
		}
		if !found {
			http.Error(w, fmt.Sprintf("Executable for '%s' not found on server", platform), http.StatusNotFound)
			return
		}
	}

	file, err := os.Open(targetFile)
	if err != nil {
		http.Error(w, "Error opening release file", http.StatusInternalServerError)
		return
	}
	defer file.Close()

	stat, _ := file.Stat()
	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=\"%s\"", cleanPlatform))
	w.Header().Set("Content-Type", "application/octet-stream")
	w.Header().Set("Content-Length", fmt.Sprintf("%d", stat.Size()))
	w.Header().Set("Cache-Control", "public, max-age=3600")

	io.Copy(w, file)
}

func (s *Server) handleUIVersions(w http.ResponseWriter, r *http.Request) {
	baseURL := s.getBaseURL(r)
	s.mu.RLock()
	defer s.mu.RUnlock()

	platforms := []string{
		"craft-studio-linux-amd64.tar.gz",
		"craft-studio-linux-arm64.tar.gz",
		"craft-studio-darwin-arm64.tar.gz",
		"craft-studio-darwin-amd64.tar.gz",
		"craft-studio-windows-amd64.zip",
	}

	assets := make(map[string]any)
	for _, p := range platforms {
		filePath := filepath.Join(s.config.ReleasesDir, p)
		info, err := os.Stat(filePath)
		var size int64 = 0
		available := false
		if err == nil && !info.IsDir() {
			size = info.Size()
			available = true
		}

		assets[p] = map[string]any{
			"url":       fmt.Sprintf("%s/download/ui/%s", baseURL, p),
			"size":      size,
			"available": available,
			"sha256":    s.checksums[p],
		}
	}

	resp := map[string]any{
		"version":      AppVersion,
		"release_date": "2026-09-23",
		"notes":        "Craft Desktop Studio standalone release - graphical server manager with embedded CLI companion",
		"assets":       assets,
	}
	writeJSON(w, http.StatusOK, resp)
}

func (s *Server) handleDownloadUI(w http.ResponseWriter, r *http.Request) {
	platform := r.URL.Query().Get("platform")
	if platform == "" {
		ua := strings.ToLower(r.UserAgent())
		switch {
		case strings.Contains(ua, "darwin") || strings.Contains(ua, "macintosh") || strings.Contains(ua, "mac os"):
			if strings.Contains(ua, "arm64") || strings.Contains(ua, "aarch64") {
				platform = "darwin-arm64"
			} else {
				platform = "darwin-arm64"
			}
		case strings.Contains(ua, "windows"):
			platform = "windows-amd64"
		default:
			platform = "linux-amd64"
		}
	}

	s.serveUIArchive(w, r, platform)
}

func (s *Server) handleDownloadUIPlatform(w http.ResponseWriter, r *http.Request) {
	platform := r.PathValue("platform")
	if platform == "" {
		http.Error(w, "Platform parameter required", http.StatusBadRequest)
		return
	}
	s.serveUIArchive(w, r, platform)
}

func (s *Server) serveUIArchive(w http.ResponseWriter, r *http.Request, platform string) {
	cleanPlatform := filepath.Base(platform)
	targetFile := filepath.Join(s.config.ReleasesDir, cleanPlatform)

	if _, err := os.Stat(targetFile); os.IsNotExist(err) {
		candidates := []string{
			cleanPlatform,
			"craft-studio-" + cleanPlatform + ".tar.gz",
			"craft-studio-" + cleanPlatform + ".zip",
			"craft-studio-" + cleanPlatform,
			cleanPlatform + ".tar.gz",
			cleanPlatform + ".zip",
			"craft-studio-linux-amd64.tar.gz",
		}
		found := false
		for _, cand := range candidates {
			candPath := filepath.Join(s.config.ReleasesDir, cand)
			if _, err := os.Stat(candPath); err == nil {
				targetFile = candPath
				cleanPlatform = cand
				found = true
				break
			}
		}
		if !found {
			http.Error(w, fmt.Sprintf("Craft Studio archive for '%s' not found on server", platform), http.StatusNotFound)
			return
		}
	}

	file, err := os.Open(targetFile)
	if err != nil {
		http.Error(w, "Error opening release file", http.StatusInternalServerError)
		return
	}
	defer file.Close()

	stat, _ := file.Stat()
	contentType := "application/octet-stream"
	if strings.HasSuffix(cleanPlatform, ".tar.gz") {
		contentType = "application/gzip"
	} else if strings.HasSuffix(cleanPlatform, ".zip") {
		contentType = "application/zip"
	}

	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=\"%s\"", cleanPlatform))
	w.Header().Set("Content-Type", contentType)
	w.Header().Set("Content-Length", fmt.Sprintf("%d", stat.Size()))
	w.Header().Set("Cache-Control", "public, max-age=3600")

	io.Copy(w, file)
}

func (s *Server) handleInstallSh(w http.ResponseWriter, r *http.Request) {
	baseURL := s.getBaseURL(r)
	script := strings.ReplaceAll(installShTemplate, "{{BASE_URL}}", baseURL)
	w.Header().Set("Content-Type", "text/x-shellscript; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Write([]byte(script))
}

func (s *Server) handleInstallPs1(w http.ResponseWriter, r *http.Request) {
	baseURL := s.getBaseURL(r)
	script := strings.ReplaceAll(installPs1Template, "{{BASE_URL}}", baseURL)
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Write([]byte(script))
}

func (s *Server) getBaseURL(r *http.Request) string {
	if s.config.PublicURL != "" {
		return strings.TrimRight(s.config.PublicURL, "/")
	}
	proto := "http"
	if r.TLS != nil || r.Header.Get("X-Forwarded-Proto") == "https" {
		proto = "https"
	}
	return fmt.Sprintf("%s://%s", proto, r.Host)
}

func (s *Server) refreshChecksums() {
	s.mu.Lock()
	defer s.mu.Unlock()

	entries, err := os.ReadDir(s.config.ReleasesDir)
	if err != nil {
		return
	}

	for _, entry := range entries {
		if entry.IsDir() || strings.HasSuffix(entry.Name(), ".sha256") {
			continue
		}
		path := filepath.Join(s.config.ReleasesDir, entry.Name())
		hash, err := computeSHA256(path)
		if err == nil {
			s.checksums[entry.Name()] = hash
			_ = os.WriteFile(path+".sha256", []byte(hash+"  "+entry.Name()+"\n"), 0644)
		}
	}
}

func computeSHA256(filePath string) (string, error) {
	file, err := os.Open(filePath)
	if err != nil {
		return "", err
	}
	defer file.Close()

	hasher := sha256.New()
	if _, err := io.Copy(hasher, file); err != nil {
		return "", err
	}
	return hex.EncodeToString(hasher.Sum(nil)), nil
}

func writeJSON(w http.ResponseWriter, status int, data any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(data)
}

func getEnv(key, defaultVal string) string {
	if val, ok := os.LookupEnv(key); ok && val != "" {
		return val
	}
	return defaultVal
}
