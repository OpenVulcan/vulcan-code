package streammux

import (
	"context"
	"io"
	"testing"
)

type memSender struct{ dst *Mux }

func (m memSender) Send(b []byte) error { return m.dst.HandleMessage(b) }
func TestMuxOpenDataClose(t *testing.T) {
	var a, b *Mux
	a = New(memSender{}, true)
	b = New(memSender{}, false)
	a.sender = memSender{b}
	b.sender = memSender{a}
	s, err := a.Open(context.Background(), OpenPayload{Proto: "http", Path: "/status"})
	if err != nil {
		t.Fatal(err)
	}
	rs, err := b.Accept(context.Background())
	if err != nil || rs.Open.Path != "/status" {
		t.Fatalf("accept %#v %v", rs, err)
	}
	_, _ = s.Write([]byte("hello"))
	buf := make([]byte, 5)
	n, err := io.ReadFull(rs, buf)
	if err != nil || n != 5 || string(buf) != "hello" {
		t.Fatalf("read %d %q %v", n, buf, err)
	}
	_ = s.Close()
	_, err = rs.Read(buf)
	if err != io.EOF {
		t.Fatalf("want eof got %v", err)
	}
}
