package proxy

import "errors"

type WSOpcode byte

const (
	WSContinuation WSOpcode = 0
	WSText         WSOpcode = 1
	WSBinary       WSOpcode = 2
	WSClose        WSOpcode = 8
	WSPing         WSOpcode = 9
	WSPong         WSOpcode = 10
)

func EncodeWSFrame(op WSOpcode, payload []byte) []byte {
	b := make([]byte, 1+len(payload))
	b[0] = byte(op)
	copy(b[1:], payload)
	return b
}
func DecodeWSFrame(b []byte) (WSOpcode, []byte, error) {
	if len(b) == 0 {
		return 0, nil, errors.New("empty websocket frame")
	}
	return WSOpcode(b[0]), b[1:], nil
}
