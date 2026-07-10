package relay

import (
	"crypto/hmac"
	"crypto/sha1"
	"encoding/base64"
	"fmt"
	"strconv"
	"strings"
	"time"
)

type TURNCredential struct {
	Username   string    `json:"username"`
	Credential string    `json:"credential"`
	ExpiresAt  time.Time `json:"expires_at"`
}

func NewTURNCredential(secret []byte, userID, sessionID string, ttl time.Duration) TURNCredential {
	exp := time.Now().Add(ttl).UTC()
	u := fmt.Sprintf("%s:%s:%d", userID, sessionID, exp.Unix())
	mac := hmac.New(sha1.New, secret)
	mac.Write([]byte(u))
	return TURNCredential{Username: u, Credential: base64.StdEncoding.EncodeToString(mac.Sum(nil)), ExpiresAt: exp}
}
func ValidateTURNCredential(secret []byte, username, credential string, now time.Time) bool {
	parts := strings.Split(username, ":")
	if len(parts) != 3 {
		return false
	}
	unix, err := strconv.ParseInt(parts[2], 10, 64)
	if err != nil || now.After(time.Unix(unix, 0)) {
		return false
	}
	expected := NewTURNCredentialAt(secret, username)
	return hmac.Equal([]byte(expected), []byte(credential))
}
func NewTURNCredentialAt(secret []byte, username string) string {
	mac := hmac.New(sha1.New, secret)
	mac.Write([]byte(username))
	return base64.StdEncoding.EncodeToString(mac.Sum(nil))
}
