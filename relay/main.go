// ARKON Relay Server
//
// A minimal Go HTTP server that acts as a signalling relay for the ARKON P2P preview.
// It maintains a registry of peer_id → {local_addr, token, expires_at} and proxies
// WebSocket connections between browsers and the ARKON preview node.
//
// Usage:
//   go run main.go --port 8080 --ttl 86400
//
// Deploy on any $5/mo VPS. Set ARKON_RELAY_URL=https://your-relay.com in arkon.toml
// to use your own relay instead of the public one.
//
// Protocol:
//   POST /register   { peer_id, token, local_addr, ttl_secs }  → { ok, public_url }
//   DELETE /register { peer_id, token }                         → { ok }
//   GET  /p/:peer_id (public, browser)                         → WebSocket proxy
//   GET  /health                                               → { ok, peers }

package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"os"
	"sync"
	"time"
)

// ── Peer registry ─────────────────────────────────────────────────────────────

type PeerEntry struct {
	LocalAddr  string
	Token      string
	ExpiresAt  time.Time
}

type Registry struct {
	mu    sync.RWMutex
	peers map[string]*PeerEntry
}

func NewRegistry() *Registry {
	r := &Registry{peers: make(map[string]*PeerEntry)}
	go r.sweepLoop()
	return r
}

func (r *Registry) Register(peerID, token, localAddr string, ttlSecs int) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.peers[peerID] = &PeerEntry{
		LocalAddr: localAddr,
		Token:     token,
		ExpiresAt: time.Now().Add(time.Duration(ttlSecs) * time.Second),
	}
	log.Printf("registered peer %s → %s (ttl %ds)", peerID, localAddr, ttlSecs)
}

func (r *Registry) Deregister(peerID, token string) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	entry, ok := r.peers[peerID]
	if !ok || entry.Token != token {
		return false
	}
	delete(r.peers, peerID)
	log.Printf("deregistered peer %s", peerID)
	return true
}

func (r *Registry) Get(peerID string) (*PeerEntry, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	e, ok := r.peers[peerID]
	if !ok || time.Now().After(e.ExpiresAt) {
		return nil, false
	}
	return e, true
}

func (r *Registry) Count() int {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return len(r.peers)
}

func (r *Registry) sweepLoop() {
	for range time.Tick(60 * time.Second) {
		r.mu.Lock()
		now := time.Now()
		for id, e := range r.peers {
			if now.After(e.ExpiresAt) {
				delete(r.peers, id)
				log.Printf("expired peer %s", id)
			}
		}
		r.mu.Unlock()
	}
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

type Server struct {
	registry *Registry
	baseURL  string
}

func (s *Server) handleRegister(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodDelete {
		s.handleDeregister(w, r)
		return
	}
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		PeerID    string `json:"peer_id"`
		Token     string `json:"token"`
		LocalAddr string `json:"local_addr"`
		TTLSecs   int    `json:"ttl_secs"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request", http.StatusBadRequest)
		return
	}
	if req.PeerID == "" || req.Token == "" || req.LocalAddr == "" {
		http.Error(w, "peer_id, token, local_addr required", http.StatusBadRequest)
		return
	}
	if req.TTLSecs <= 0 || req.TTLSecs > 86400*7 {
		req.TTLSecs = 86400
	}

	s.registry.Register(req.PeerID, req.Token, req.LocalAddr, req.TTLSecs)

	publicURL := fmt.Sprintf("%s/p/%s", s.baseURL, req.PeerID)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"ok":         true,
		"public_url": publicURL,
	})
}

func (s *Server) handleDeregister(w http.ResponseWriter, r *http.Request) {
	var req struct {
		PeerID string `json:"peer_id"`
		Token  string `json:"token"`
	}
	json.NewDecoder(r.Body).Decode(&req)
	ok := s.registry.Deregister(req.PeerID, req.Token)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]bool{"ok": ok})
}

func (s *Server) handleProxy(w http.ResponseWriter, r *http.Request) {
	peerID := r.URL.Path[len("/p/"):]
	if peerID == "" {
		http.Error(w, "peer_id required", http.StatusBadRequest)
		return
	}

	entry, ok := s.registry.Get(peerID)
	if !ok {
		http.Error(w, "peer not found or expired", http.StatusNotFound)
		return
	}

	// Proxy the request to the peer's local HTTP server
	targetURL := fmt.Sprintf("http://%s%s", entry.LocalAddr, r.URL.Path[len("/p/"+peerID):])
	if r.URL.RawQuery != "" {
		targetURL += "?" + r.URL.RawQuery
	}

	proxyReq, err := http.NewRequest(r.Method, targetURL, r.Body)
	if err != nil {
		http.Error(w, "proxy error", http.StatusBadGateway)
		return
	}
	for k, vs := range r.Header {
		for _, v := range vs {
			proxyReq.Header.Add(k, v)
		}
	}

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(proxyReq)
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream unreachable: %v", err), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()

	for k, vs := range resp.Header {
		for _, v := range vs {
			w.Header().Add(k, v)
		}
	}
	w.Header().Set("X-Arkon-Relay", "1")
	w.WriteHeader(resp.StatusCode)
	io.Copy(w, resp.Body)
}

func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"ok":    true,
		"peers": s.registry.Count(),
		"ts":    time.Now().UTC().Format(time.RFC3339),
	})
}

// ── Main ──────────────────────────────────────────────────────────────────────

func main() {
	port    := flag.Int("port", 8080, "port to listen on")
	baseURL := flag.String("base-url", "", "public base URL (e.g. https://relay.arkon.sh)")
	flag.Parse()

	// Render (and most PaaS) injects PORT env var — honour it
	if envPort := os.Getenv("PORT"); envPort != "" {
		fmt.Sscanf(envPort, "%d", port)
	}

	if *baseURL == "" {
		if envURL := os.Getenv("BASE_URL"); envURL != "" {
			*baseURL = envURL
		} else {
			*baseURL = fmt.Sprintf("http://localhost:%d", *port)
		}
	}

	registry := NewRegistry()
	srv := &Server{registry: registry, baseURL: *baseURL}

	mux := http.NewServeMux()
	mux.HandleFunc("/register", srv.handleRegister)
	mux.HandleFunc("/p/",       srv.handleProxy)
	mux.HandleFunc("/health",   srv.handleHealth)

	addr := fmt.Sprintf(":%d", *port)
	log.Printf("ARKON relay listening on %s (base: %s)", addr, *baseURL)
	log.Fatal(http.ListenAndServe(addr, mux))
}
