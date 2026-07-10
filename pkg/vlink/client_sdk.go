package vlink

const Version = "0.1.0"

type Client struct {
	RelayURL       string
	RemoteDeviceID string
}

func NewClient(relayURL, remoteDeviceID string) *Client {
	return &Client{RelayURL: relayURL, RemoteDeviceID: remoteDeviceID}
}
