package streammux

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"sync"
)

type Sender interface{ Send([]byte) error }

type OpenPayload struct {
	Proto, Method, Path, Query string
	Headers                    map[string][]string
}
type ResponseHeader struct {
	Status  int                 `json:"status"`
	Headers map[string][]string `json:"headers"`
}

type Mux struct {
	sender   Sender
	next     uint64
	mu       sync.Mutex
	streams  map[uint64]*Stream
	incoming chan *Stream
	closed   chan struct{}
}

func New(sender Sender, clientInitiated bool) *Mux {
	start := uint64(2)
	if clientInitiated {
		start = 1
	}
	return &Mux{sender: sender, next: start, streams: map[uint64]*Stream{}, incoming: make(chan *Stream, 64), closed: make(chan struct{})}
}
func (m *Mux) Open(ctx context.Context, p OpenPayload) (*Stream, error) {
	b, err := json.Marshal(p)
	if err != nil {
		return nil, err
	}
	id := m.alloc()
	s := newStream(id, m)
	m.mu.Lock()
	m.streams[id] = s
	m.mu.Unlock()
	if err := m.send(Frame{Type: FrameOpen, StreamID: id, Payload: b}); err != nil {
		return nil, err
	}
	return s, nil
}
func (m *Mux) Accept(ctx context.Context) (*Stream, error) {
	select {
	case s := <-m.incoming:
		return s, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-m.closed:
		return nil, io.ErrClosedPipe
	}
}
func (m *Mux) HandleMessage(b []byte) error {
	f, err := Decode(b)
	if err != nil {
		return err
	}
	if f.Type == FramePing {
		return m.send(Frame{Type: FramePong, StreamID: f.StreamID, Payload: f.Payload})
	}
	m.mu.Lock()
	s := m.streams[f.StreamID]
	if f.Type == FrameOpen && s == nil {
		s = newStream(f.StreamID, m)
		if json.Unmarshal(f.Payload, &s.Open) != nil {
			m.mu.Unlock()
			return errors.New("bad open payload")
		}
		m.streams[f.StreamID] = s
		m.mu.Unlock()
		m.incoming <- s
		return nil
	}
	m.mu.Unlock()
	if s == nil {
		return nil
	}
	switch f.Type {
	case FrameData:
		s.deliver(f.Payload)
	case FrameClose:
		s.closeRead()
	case FrameReset:
		s.reset()
	}
	return nil
}
func (m *Mux) Ping(payload []byte) error { return m.send(Frame{Type: FramePing, Payload: payload}) }
func (m *Mux) send(f Frame) error        { return m.sender.Send(Encode(f)) }
func (m *Mux) alloc() uint64             { m.mu.Lock(); defer m.mu.Unlock(); id := m.next; m.next += 2; return id }
func (m *Mux) remove(id uint64)          { m.mu.Lock(); delete(m.streams, id); m.mu.Unlock() }

type Stream struct {
	ID     uint64
	Open   OpenPayload
	mux    *Mux
	read   chan []byte
	once   sync.Once
	closed chan struct{}
	buf    []byte
	mu     sync.Mutex
}

func newStream(id uint64, m *Mux) *Stream {
	return &Stream{ID: id, mux: m, read: make(chan []byte, 64), closed: make(chan struct{})}
}
func (s *Stream) Write(p []byte) (int, error) {
	cp := append([]byte(nil), p...)
	if err := s.mux.send(Frame{Type: FrameData, StreamID: s.ID, Payload: cp}); err != nil {
		return 0, err
	}
	return len(p), nil
}
func (s *Stream) Read(p []byte) (int, error) {
	for len(s.buf) == 0 {
		b, ok := <-s.read
		if !ok {
			return 0, io.EOF
		}
		s.buf = b
	}
	n := copy(p, s.buf)
	s.buf = s.buf[n:]
	return n, nil
}
func (s *Stream) CloseWrite() error { return s.mux.send(Frame{Type: FrameClose, StreamID: s.ID}) }
func (s *Stream) Close() error {
	s.once.Do(func() { _ = s.mux.send(Frame{Type: FrameClose, StreamID: s.ID}); close(s.closed); s.mux.remove(s.ID) })
	return nil
}
func (s *Stream) deliver(b []byte) {
	select {
	case s.read <- append([]byte(nil), b...):
	default:
	}
}
func (s *Stream) closeRead() { close(s.read) }
func (s *Stream) reset()     { close(s.read); s.once.Do(func() { close(s.closed); s.mux.remove(s.ID) }) }
func (s *Stream) SendJSON(v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	_, err = s.Write(b)
	return err
}
func (s *Stream) ReadJSON(v any) error {
	b, err := io.ReadAll(s)
	if err != nil {
		return err
	}
	return json.Unmarshal(b, v)
}
