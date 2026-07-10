package webrtc

import (
	"context"

	pion "github.com/pion/webrtc/v4"
)

func CreateOffer(ctx context.Context, pc *pion.PeerConnection) (pion.SessionDescription, error) {
	offer, err := pc.CreateOffer(nil)
	if err != nil {
		return pion.SessionDescription{}, err
	}
	if err = pc.SetLocalDescription(offer); err != nil {
		return pion.SessionDescription{}, err
	}
	<-pion.GatheringCompletePromise(pc)
	return *pc.LocalDescription(), nil
}
func AcceptOffer(ctx context.Context, pc *pion.PeerConnection, offer pion.SessionDescription) (pion.SessionDescription, error) {
	if err := pc.SetRemoteDescription(offer); err != nil {
		return pion.SessionDescription{}, err
	}
	answer, err := pc.CreateAnswer(nil)
	if err != nil {
		return pion.SessionDescription{}, err
	}
	if err = pc.SetLocalDescription(answer); err != nil {
		return pion.SessionDescription{}, err
	}
	<-pion.GatheringCompletePromise(pc)
	return *pc.LocalDescription(), nil
}
func AcceptAnswer(pc *pion.PeerConnection, answer pion.SessionDescription) error {
	return pc.SetRemoteDescription(answer)
}
func CreateVLinkDataChannel(pc *pion.PeerConnection) (*pion.DataChannel, error) {
	ordered := true
	return pc.CreateDataChannel("vlink", &pion.DataChannelInit{Ordered: &ordered})
}
