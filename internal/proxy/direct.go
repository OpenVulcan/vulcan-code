package proxy

import (
	"net/http"
	"net/http/httputil"
	"net/url"
	"strings"
)

type DirectProxy struct {
	target *url.URL
	proxy  *httputil.ReverseProxy
}

func NewDirectProxy(target string) (*DirectProxy, error) {
	u, err := url.Parse(target)
	if err != nil {
		return nil, err
	}
	p := httputil.NewSingleHostReverseProxy(u)
	return &DirectProxy{target: u, proxy: p}, nil
}
func (p *DirectProxy) ListenAndServe(addr string) error { return http.ListenAndServe(addr, p) }
func (p *DirectProxy) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	if isWS(r) {
		p.proxy.ServeHTTP(w, r)
		return
	}
	p.proxy.ServeHTTP(w, r)
}
func isWS(r *http.Request) bool {
	return strings.EqualFold(r.Header.Get("Upgrade"), "websocket") && strings.Contains(strings.ToLower(r.Header.Get("Connection")), "upgrade")
}
