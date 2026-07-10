package proxy

import "testing"

func TestWSFrame(t *testing.T) {
	op, p, err := DecodeWSFrame(EncodeWSFrame(WSText, []byte("hi")))
	if err != nil || op != WSText || string(p) != "hi" {
		t.Fatalf("%v %q %v", op, p, err)
	}
}
