package identity

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base32"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

type Device struct{ ID, Name, PublicKeyBase64 string }

type keyFile struct {
	Name       string    `json:"name"`
	PublicKey  string    `json:"public_key"`
	PrivateKey string    `json:"private_key"`
	CreatedAt  time.Time `json:"created_at"`
}

func DefaultKeyPath() string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".vulcan-link", "device_ed25519.json")
}

func InitDevice(name, path string) (Device, error) {
	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		return Device{}, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return Device{}, err
	}
	kf := keyFile{Name: name, PublicKey: base64.StdEncoding.EncodeToString(pub), PrivateKey: base64.StdEncoding.EncodeToString(priv), CreatedAt: time.Now().UTC()}
	b, _ := json.MarshalIndent(kf, "", "  ")
	if err := os.WriteFile(path, b, 0o600); err != nil {
		return Device{}, err
	}
	return Device{ID: DeviceID(pub), Name: name, PublicKeyBase64: kf.PublicKey}, nil
}

func DeviceID(pub ed25519.PublicKey) string {
	sum := sha256.Sum256(pub)
	enc := base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(sum[:16])
	return "dev_" + strings.ToLower(enc)
}

func Sign(priv ed25519.PrivateKey, payload []byte) []byte {
	h := sha256.Sum256(payload)
	return ed25519.Sign(priv, h[:])
}
func Verify(pub ed25519.PublicKey, payload, sig []byte) bool {
	h := sha256.Sum256(payload)
	return ed25519.Verify(pub, h[:], sig)
}

func NewPairCode(deviceID, publicKey string) (string, error) {
	token := make([]byte, 16)
	if _, err := rand.Read(token); err != nil {
		return "", err
	}
	return fmt.Sprintf("VLK1:%s:%s:%s", deviceID, publicKey, base64.RawURLEncoding.EncodeToString(token)), nil
}
