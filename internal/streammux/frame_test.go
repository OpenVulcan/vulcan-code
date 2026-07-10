package streammux

import "testing"

func TestFrameRoundTrip(t *testing.T) {
	in := Frame{Type: FrameData, StreamID: 7, Payload: []byte("abc")}
	out, err := Decode(Encode(in))
	if err != nil || out.Type != in.Type || out.StreamID != 7 || string(out.Payload) != "abc" {
		t.Fatalf("bad roundtrip %#v %v", out, err)
	}
}
