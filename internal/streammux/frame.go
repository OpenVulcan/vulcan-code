package streammux

import (
	"encoding/binary"
	"errors"
	"fmt"
)

var magic = [4]byte{'V', 'L', 'K', '1'}

const Version byte = 1
const HeaderLen = 18

type FrameType byte

const (
	FrameOpen     FrameType = 1
	FrameData     FrameType = 2
	FrameClose    FrameType = 3
	FrameReset    FrameType = 4
	FramePing     FrameType = 5
	FramePong     FrameType = 6
	FrameAuth     FrameType = 7
	FrameAuthOK   FrameType = 8
	FrameAuthFail FrameType = 9
)

type Frame struct {
	Type     FrameType
	StreamID uint64
	Payload  []byte
}

func Encode(f Frame) []byte {
	b := make([]byte, HeaderLen+len(f.Payload))
	copy(b[:4], magic[:])
	b[4] = Version
	b[5] = byte(f.Type)
	binary.BigEndian.PutUint64(b[6:14], f.StreamID)
	binary.BigEndian.PutUint32(b[14:18], uint32(len(f.Payload)))
	copy(b[18:], f.Payload)
	return b
}
func Decode(b []byte) (Frame, error) {
	if len(b) < HeaderLen {
		return Frame{}, errors.New("short frame")
	}
	if string(b[:4]) != "VLK1" {
		return Frame{}, errors.New("bad magic")
	}
	if b[4] != Version {
		return Frame{}, fmt.Errorf("unsupported version %d", b[4])
	}
	n := binary.BigEndian.Uint32(b[14:18])
	if int(n) != len(b)-HeaderLen {
		return Frame{}, errors.New("length mismatch")
	}
	return Frame{Type: FrameType(b[5]), StreamID: binary.BigEndian.Uint64(b[6:14]), Payload: b[18:]}, nil
}
