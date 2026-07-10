package signaling

import (
	"context"
	"encoding/json"
	"net/url"
	"strings"
	"sync"

	"github.com/gobwas/ws"
	"github.com/gobwas/ws/wsutil"
	"github.com/vulcancode/vulcan-link/internal/relay"
)

type Client struct {
	conn     netConn
	mu       sync.Mutex
	incoming chan relay.SignalMessage
}
type netConn interface {
	Read([]byte) (int, error)
	Write([]byte) (int, error)
	Close() error
}

func SignalURL(base string) (string, error) {
	u, err := url.Parse(base)
	if err != nil {
		return "", err
	}
	switch u.Scheme {
	case "http":
		u.Scheme = "ws"
	case "https":
		u.Scheme = "wss"
	}
	if !strings.HasSuffix(u.Path, "/v1/signal") {
		u.Path = strings.TrimRight(u.Path, "/") + "/v1/signal"
	}
	return u.String(), nil
}
func Dial(ctx context.Context, relayURL string) (*Client, error) {
	su, err := SignalURL(relayURL)
	if err != nil {
		return nil, err
	}
	conn, _, _, err := ws.Dial(ctx, su)
	if err != nil {
		return nil, err
	}
	c := &Client{conn: conn, incoming: make(chan relay.SignalMessage, 64)}
	go c.readLoop()
	return c, nil
}
func (c *Client) Register(deviceID, name, publicKey, sig string, ts int64) error {
	return c.Send(relay.SignalMessage{Type: "device_register", DeviceID: deviceID, DeviceName: name, PublicKey: publicKey, Signature: sig, Timestamp: ts})
}
func (c *Client) Send(m relay.SignalMessage) error {
	b, err := json.Marshal(m)
	if err != nil {
		return err
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return wsutil.WriteClientText(c.conn, b)
}
func (c *Client) Recv(ctx context.Context) (relay.SignalMessage, error) {
	select {
	case m := <-c.incoming:
		return m, nil
	case <-ctx.Done():
		return relay.SignalMessage{}, ctx.Err()
	}
}
func (c *Client) Close() error { return c.conn.Close() }
func (c *Client) readLoop() {
	defer close(c.incoming)
	for {
		b, _, err := wsutil.ReadServerData(c.conn)
		if err != nil {
			return
		}
		var m relay.SignalMessage
		if json.Unmarshal(b, &m) == nil {
			c.incoming <- m
		}
	}
}
