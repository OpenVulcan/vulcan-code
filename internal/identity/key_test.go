package identity

import (
	"crypto/ed25519"
	"crypto/rand"
	"strings"
	"testing"
)

func TestDeviceIDAndSignature(t *testing.T) {
	pub, priv, _ := ed25519.GenerateKey(rand.Reader)
	if !strings.HasPrefix(DeviceID(pub), "dev_") {
		t.Fatal("missing prefix")
	}
	msg := []byte("hello")
	sig := Sign(priv, msg)
	if !Verify(pub, msg, sig) {
		t.Fatal("signature did not verify")
	}
}
