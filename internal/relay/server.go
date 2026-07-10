package relay

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"sync"
	"time"

	"github.com/gobwas/ws"
	"github.com/gobwas/ws/wsutil"
)

type SignalMessage struct {
	Type         string `json:"type"`
	DeviceID     string `json:"device_id,omitempty"`
	DeviceName   string `json:"device_name,omitempty"`
	SessionID    string `json:"session_id,omitempty"`
	FromDeviceID string `json:"from_device_id,omitempty"`
	ToDeviceID   string `json:"to_device_id,omitempty"`
	SDP          string `json:"sdp,omitempty"`
	Candidate    string `json:"candidate,omitempty"`
	PublicKey    string `json:"public_key,omitempty"`
	Timestamp    int64  `json:"timestamp,omitempty"`
	Signature    string `json:"signature,omitempty"`
}

type Server struct {
	domain  string
	secret  []byte
	mu      sync.RWMutex
	devices map[string]*peer
}
type peer struct {
	id       string
	send     chan SignalMessage
	lastSeen time.Time
}

func NewServer(domain string, secret []byte) *Server {
	return &Server{domain: domain, secret: secret, devices: map[string]*peer{}}
}
func (s *Server) ListenAndServe(addr string) error {
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) { w.Write([]byte("ok")) })
	mux.HandleFunc("/v1/turn-credentials", s.turnCreds)
	mux.HandleFunc("/v1/signal", s.signal)
	return http.ListenAndServe(addr, mux)
}
func (s *Server) turnCreds(w http.ResponseWriter, r *http.Request) {
	c := NewTURNCredential(s.secret, r.URL.Query().Get("user_id"), r.URL.Query().Get("session_id"), time.Hour)
	_ = json.NewEncoder(w).Encode(c)
}
func (s *Server) signal(w http.ResponseWriter, r *http.Request) {
	conn, _, _, err := ws.UpgradeHTTP(r, w)
	if err != nil {
		return
	}
	defer conn.Close()
	var id string
	send := make(chan SignalMessage, 16)
	done := make(chan struct{})
	go func() {
		defer close(done)
		for m := range send {
			b, _ := json.Marshal(m)
			_ = wsutil.WriteServerText(conn, b)
		}
	}()
	for {
		b, _, err := wsutil.ReadClientData(conn)
		if err != nil {
			break
		}
		var m SignalMessage
		if json.Unmarshal(b, &m) != nil {
			continue
		}
		switch m.Type {
		case "device_register":
			id = m.DeviceID
			s.mu.Lock()
			s.devices[id] = &peer{id: id, send: send, lastSeen: time.Now()}
			s.mu.Unlock()
			send <- SignalMessage{Type: "device_registered", DeviceID: id}
			log.Printf("device online: %s", id)
		case "connect_request", "sdp_offer", "sdp_answer", "ice_candidate":
			s.forward(m)
		}
	}
	if id != "" {
		s.mu.Lock()
		delete(s.devices, id)
		s.mu.Unlock()
	}
	close(send)
	<-done
}
func (s *Server) forward(m SignalMessage) {
	to := m.ToDeviceID
	if to == "" {
		return
	}
	s.mu.RLock()
	p := s.devices[to]
	s.mu.RUnlock()
	if p != nil {
		select {
		case p.send <- m:
		default:
		}
	}
}
func (s *Server) ListenAndServeTURN(ctx context.Context, addr string) error {
	<-ctx.Done()
	return ctx.Err()
}
