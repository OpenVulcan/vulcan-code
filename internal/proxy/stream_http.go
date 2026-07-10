package proxy

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"

	"github.com/vulcancode/vulcan-link/internal/streammux"
)

type StreamClientProxy struct{ mux *streammux.Mux }

func NewStreamClientProxy(m *streammux.Mux) *StreamClientProxy { return &StreamClientProxy{mux: m} }
func (p *StreamClientProxy) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	s, err := p.mux.Open(r.Context(), streammux.OpenPayload{Proto: "http", Method: r.Method, Path: r.URL.Path, Query: r.URL.RawQuery, Headers: r.Header})
	if err != nil {
		http.Error(w, err.Error(), 502)
		return
	}
	defer s.Close()
	go func() { _, _ = io.Copy(s, r.Body); _ = r.Body.Close(); _ = s.CloseWrite() }()
	dec := json.NewDecoder(s)
	var h streammux.ResponseHeader
	if err := dec.Decode(&h); err != nil {
		http.Error(w, err.Error(), 502)
		return
	}
	for k, vs := range h.Headers {
		for _, v := range vs {
			w.Header().Add(k, v)
		}
	}
	if h.Status == 0 {
		h.Status = 200
	}
	w.WriteHeader(h.Status)
	_, _ = io.Copy(w, dec.Buffered())
	_, _ = io.Copy(w, s)
}

type StreamAgentProxy struct {
	Target *url.URL
	Client *http.Client
}

func NewStreamAgentProxy(target string) (*StreamAgentProxy, error) {
	u, err := url.Parse(target)
	if err != nil {
		return nil, err
	}
	return &StreamAgentProxy{Target: u, Client: http.DefaultClient}, nil
}
func (p *StreamAgentProxy) Serve(ctx context.Context, m *streammux.Mux) error {
	for {
		s, err := m.Accept(ctx)
		if err != nil {
			return err
		}
		go p.handle(s)
	}
}
func (p *StreamAgentProxy) handle(s *streammux.Stream) {
	defer s.Close()
	if s.Open.Proto != "http" {
		return
	}
	u := *p.Target
	u.Path = joinPath(p.Target.Path, s.Open.Path)
	u.RawQuery = s.Open.Query
	req, err := http.NewRequest(s.Open.Method, u.String(), s)
	if err != nil {
		_ = s.SendJSON(streammux.ResponseHeader{Status: 502})
		return
	}
	req.Header = s.Open.Headers
	resp, err := p.Client.Do(req)
	if err != nil {
		_ = s.SendJSON(streammux.ResponseHeader{Status: 502, Headers: map[string][]string{"x-vlink-error": {err.Error()}}})
		return
	}
	defer resp.Body.Close()
	_ = s.SendJSON(streammux.ResponseHeader{Status: resp.StatusCode, Headers: resp.Header})
	_, _ = io.Copy(s, resp.Body)
}
func joinPath(base, p string) string {
	if base == "" || base == "/" {
		return p
	}
	if p == "" || p == "/" {
		return base
	}
	if base[len(base)-1] == '/' && p[0] == '/' {
		return base + p[1:]
	}
	if base[len(base)-1] != '/' && p[0] != '/' {
		return base + "/" + p
	}
	return base + p
}
