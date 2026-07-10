package webrtc

import pion "github.com/pion/webrtc/v4"

type ICEConfig struct {
	STUNURLs   []string
	TURNURLs   []string
	Username   string
	Credential string
}

func Configuration(c ICEConfig) pion.Configuration {
	servers := []pion.ICEServer{}
	if len(c.STUNURLs) > 0 {
		servers = append(servers, pion.ICEServer{URLs: c.STUNURLs})
	}
	if len(c.TURNURLs) > 0 {
		servers = append(servers, pion.ICEServer{URLs: c.TURNURLs, Username: c.Username, Credential: c.Credential})
	}
	return pion.Configuration{ICEServers: servers}
}
func NewPeer(c ICEConfig) (*pion.PeerConnection, error) {
	return pion.NewPeerConnection(Configuration(c))
}
