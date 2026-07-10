package relay

import (
	"testing"
	"time"
)

func TestTURNCredential(t *testing.T) {
	c := NewTURNCredential([]byte("s"), "u", "sess", time.Hour)
	if !ValidateTURNCredential([]byte("s"), c.Username, c.Credential, time.Now()) {
		t.Fatal("valid credential rejected")
	}
}
