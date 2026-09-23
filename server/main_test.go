package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func setupTestServer(t *testing.T) (*Server, string) {
	t.Helper()
	tempDir := t.TempDir()

	// Create mock release files
	cliBinary := filepath.Join(tempDir, "craft-linux-amd64")
	if err := os.WriteFile(cliBinary, []byte("mock-craft-cli-binary"), 0755); err != nil {
		t.Fatalf("Failed to write mock cli binary: %v", err)
	}

	uiArchive := filepath.Join(tempDir, "craft-studio-linux-amd64.tar.gz")
	if err := os.WriteFile(uiArchive, []byte("mock-craft-studio-tarball-archive"), 0644); err != nil {
		t.Fatalf("Failed to write mock ui archive: %v", err)
	}

	srv := &Server{
		config: ServerConfig{
			Port:        "8080",
			ReleasesDir: tempDir,
			PublicURL:   "http://localhost:8080",
		},
		startTime: time.Now(),
		checksums: make(map[string]string),
	}
	srv.refreshChecksums()
	return srv, tempDir
}

func TestHealthz(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/healthz", nil)
	rec := httptest.NewRecorder()

	srv.handleHealthz(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	var resp map[string]any
	if err := json.NewDecoder(rec.Body).Decode(&resp); err != nil {
		t.Fatalf("Failed to decode JSON: %v", err)
	}

	if resp["status"] != "healthy" {
		t.Errorf("Expected status healthy, got %v", resp["status"])
	}
}

func TestRoot(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/", nil)
	rec := httptest.NewRecorder()

	srv.handleRoot(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	var resp map[string]any
	if err := json.NewDecoder(rec.Body).Decode(&resp); err != nil {
		t.Fatalf("Failed to decode JSON: %v", err)
	}

	if resp["download_ui"] != "http://localhost:8080/download/ui" {
		t.Errorf("Expected download_ui link, got %v", resp["download_ui"])
	}
	if resp["api_versions_ui"] != "http://localhost:8080/api/versions/ui" {
		t.Errorf("Expected api_versions_ui link, got %v", resp["api_versions_ui"])
	}
}

func TestVersion(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/api/v1/version", nil)
	rec := httptest.NewRecorder()

	srv.handleVersion(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	var resp struct {
		Version string                    `json:"version"`
		Assets  map[string]map[string]any `json:"assets"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&resp); err != nil {
		t.Fatalf("Failed to decode JSON: %v", err)
	}

	asset, ok := resp.Assets["craft-linux-amd64"]
	if !ok {
		t.Fatalf("craft-linux-amd64 not in assets")
	}
	if asset["available"] != true {
		t.Errorf("Expected craft-linux-amd64 to be available")
	}
	if asset["sha256"] == "" {
		t.Errorf("Expected non-empty sha256 checksum")
	}
}

func TestUIVersions(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/api/versions/ui", nil)
	rec := httptest.NewRecorder()

	srv.handleUIVersions(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	var resp struct {
		Version string                    `json:"version"`
		Assets  map[string]map[string]any `json:"assets"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&resp); err != nil {
		t.Fatalf("Failed to decode JSON: %v", err)
	}

	asset, ok := resp.Assets["craft-studio-linux-amd64.tar.gz"]
	if !ok {
		t.Fatalf("craft-studio-linux-amd64.tar.gz not in assets")
	}
	if asset["available"] != true {
		t.Errorf("Expected craft-studio-linux-amd64.tar.gz to be available")
	}
	if asset["sha256"] == "" {
		t.Errorf("Expected non-empty sha256 checksum for desktop archive")
	}
}

func TestDownloadUIQueryParam(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/download/ui?platform=linux-amd64", nil)
	rec := httptest.NewRecorder()

	srv.handleDownloadUI(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	disposition := rec.Header().Get("Content-Disposition")
	if !strings.Contains(disposition, "craft-studio-linux-amd64.tar.gz") {
		t.Errorf("Expected Content-Disposition to contain craft-studio-linux-amd64.tar.gz, got %s", disposition)
	}

	body := rec.Body.String()
	if body != "mock-craft-studio-tarball-archive" {
		t.Errorf("Unexpected body content: %s", body)
	}
}

func TestDownloadUIPlatform(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/download/ui/craft-studio-linux-amd64.tar.gz", nil)
	req.SetPathValue("platform", "craft-studio-linux-amd64.tar.gz")
	rec := httptest.NewRecorder()

	srv.handleDownloadUIPlatform(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	body := rec.Body.String()
	if body != "mock-craft-studio-tarball-archive" {
		t.Errorf("Unexpected body content: %s", body)
	}
}

func TestInstallShContent(t *testing.T) {
	srv, _ := setupTestServer(t)
	req := httptest.NewRequest("GET", "/install.sh", nil)
	rec := httptest.NewRecorder()

	srv.handleInstallSh(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("Expected status 200, got %d", rec.Code)
	}

	content := rec.Body.String()
	if !strings.Contains(content, "--ui") {
		t.Errorf("Expected install.sh to contain --ui flag handling")
	}
	if !strings.Contains(content, "craft-studio") {
		t.Errorf("Expected install.sh to reference craft-studio")
	}
	if !strings.Contains(content, "http://localhost:8080") {
		t.Errorf("Expected install.sh to substitute BASE_URL")
	}
}
