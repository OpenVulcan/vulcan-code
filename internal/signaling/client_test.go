package signaling

import "testing"

func TestSignalURL(t *testing.T) {
	got, err := SignalURL("https://relay.example")
	if err != nil || got != "wss://relay.example/v1/signal" {
		t.Fatalf("%s %v", got, err)
	}
	got, err = SignalURL("http://x/base")
	if err != nil || got != "ws://x/base/v1/signal" {
		t.Fatalf("%s %v", got, err)
	}
}
